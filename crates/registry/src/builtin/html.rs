//! HTML 解析与提取：CSS 选择器、链接、纯文本。
//!
//! 爬取一个网页的第 90% 是「把响应体里的某个字段抠出来」。`codec.json.parse` 管 JSON，
//! 这里是它的 HTML 对位：输入都是字符串，输出都是能直接喂给下一个动作的值。
//!
//! 解析走 `scraper`（html5ever），因此容错与浏览器一致：畸形标签会按 HTML5 规则补全，
//! 实体（`&amp;`、`&#x4e2d;`）由分词器解码，`<table>` 里没写 `</td>` 也能选到。

use crate::ActionRegistry;
use crate::builtin::util::{opt_bool, opt_i64, opt_str, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use scraper::{ElementRef, Html, Selector, node::Node};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use url::Url;

pub struct HtmlSelect;
pub struct HtmlLinks;
pub struct HtmlText;

/// 不属于正文的标签：它们的文本节点是代码，收进来会把正文搞浑。
const NON_CONTENT_TAGS: [&str; 4] = ["script", "style", "noscript", "template"];

/// 解析文档并取出选择器；两处失败都归到「参数不对」。
fn parse(html: &str, css: &str) -> Result<(Html, Selector), ActionError> {
    let selector = Selector::parse(css)
        .map_err(|e| ActionError::InvalidParams(format!("CSS 选择器无法解析: {css}（{e:?}）")))?;
    Ok((Html::parse_document(html), selector))
}

/// 元素里的可见文本。
///
/// 不用 `ElementRef::text()`：它把 `script` / `style` 里的内联代码也当文本，
/// 正文里就会混进整段 JS。文本节点直接拼接（`Hello <b>world</b>` → `Hello world`）；
/// 栈是显式的，页面嵌套多深都不吃调用栈。
fn visible_text(root: &ElementRef) -> String {
    let mut out = String::new();
    // `Vec<_>` 让编译器去推节点句柄的具体类型，免得为此直接依赖 ego_tree。
    // 子节点一律逆序入栈：这样弹出顺序才是文档顺序。
    let mut pending: Vec<_> = root.children().rev().collect();
    while let Some(node) = pending.pop() {
        match node.value() {
            Node::Text(text) => out.push_str(&text.text),
            Node::Element(el) if NON_CONTENT_TAGS.contains(&el.name()) => {}
            _ => pending.extend(node.children().rev()),
        }
    }
    out
}

/// 一个元素抠出来的字符串：给了 `attr` 取属性，否则取它的可见文本。
fn extract(el: &ElementRef, attr: Option<&str>, trim: bool) -> Option<String> {
    let raw = match attr {
        Some(name) => el.value().attr(name)?.to_string(),
        None => visible_text(el),
    };
    Some(if trim { raw.trim().to_string() } else { raw })
}

/// 爬虫不想要的链接：空串、锚点、脚本。
fn is_junk_link(raw: &str) -> bool {
    let lowered = raw.trim().to_ascii_lowercase();
    lowered.is_empty()
        || lowered.starts_with('#')
        || lowered.starts_with("javascript:")
        || lowered.starts_with("data:")
}

/// `base` / `absolute` 这一对参数的唯一解释（两个动作共用）。
///
/// 给了 `base` 就默认绝对化，`absolute: false` 可以关掉；显式要绝对化却又不给 `base`
/// 就报错——意图明确时不该猜。返回解析好的 `Url`：整个文档只解析这一次。
fn find_base(map: &BTreeMap<String, Value>) -> Result<Option<Url>, ActionError> {
    let raw = opt_str(map, "base").filter(|s| !s.trim().is_empty());
    match (opt_bool(map, "absolute", raw.is_some()), raw) {
        (false, _) => Ok(None),
        (true, None) => Err(ActionError::MissingParam(
            "absolute: true 需要 base（用来补全相对链接）".into(),
        )),
        (true, Some(raw)) => Url::parse(&raw)
            .map(Some)
            .map_err(|e| ActionError::InvalidParams(format!("base 不是合法 URL: {e}"))),
    }
}

/// 把链接补成绝对地址；补不了就原样返回（`mailto:` / `tel:` 属于这一支）。
fn join(base: &Url, raw: &str) -> String {
    base.join(raw)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| raw.to_string())
}

/// `items` / `value` / `count` 这套输出形状与 `generate.uuid` 一致。
fn list_value(items: Vec<String>) -> Value {
    let count = items.len() as i64;
    let value = items
        .first()
        .cloned()
        .map(Value::Str)
        .unwrap_or(Value::Null);
    Value::Map(BTreeMap::from([
        (
            "items".into(),
            Value::Array(items.into_iter().map(Value::Str).collect()),
        ),
        ("count".into(), Value::Int(count)),
        ("value".into(), value),
    ]))
}

#[async_trait]
impl Action for HtmlSelect {
    fn permissions(&self) -> PermissionSet {
        // 纯字符串处理：解析、选、抠。
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "html.select",
            "HTML 选择器提取",
            "用 CSS 选择器从 HTML 里取值：元素文本或某个属性",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("html", SchemaType::Str, true)
                .with_description("HTML 文本（如 `{{steps.get.body}}`）"),
            ParamSchema::new("selector", SchemaType::Str, true)
                .with_description("CSS 选择器，如 `.title`、`a[href]`"),
            ParamSchema::new("attr", SchemaType::Str, false)
                .with_description("取这个属性而不是元素文本"),
            ParamSchema::new("all", SchemaType::Bool, false)
                .with_default(true)
                .with_description("true 取全部匹配（items）；false 只要第一个"),
            ParamSchema::new("limit", SchemaType::Int, false).with_description("最多取几个"),
            ParamSchema::new("trim", SchemaType::Bool, false)
                .with_default(true)
                .with_description("裁掉首尾空白；文本与属性值都裁"),
            ParamSchema::new("absolute", SchemaType::Bool, false).with_description(
                "给了 base 就默认绝对化；显式 false 关掉，显式 true 则必须有 base",
            ),
            ParamSchema::new("base", SchemaType::Str, false).with_description("当前页面的 URL"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let html = require_str(map, "html")?;
        let css = require_str(map, "selector")?;
        let attr = opt_str(map, "attr").filter(|s| !s.trim().is_empty());
        let take_all = opt_bool(map, "all", true);
        let trim = opt_bool(map, "trim", true);
        let limit = match opt_i64(map, "limit", 0) {
            n if n < 0 => return Err(ActionError::InvalidParams("limit 不能为负".into())),
            n => n as usize,
        };
        let base = find_base(map)?;

        let (doc, selector) = parse(&html, &css)?;
        let mut items = Vec::new();
        for el in doc.select(&selector) {
            if !take_all && !items.is_empty() {
                break;
            }
            if limit > 0 && items.len() >= limit {
                break;
            }
            let Some(mut raw) = extract(&el, attr.as_deref(), trim) else {
                continue;
            };
            if let Some(base) = &base {
                raw = join(base, &raw);
            }
            items.push(raw);
        }

        Ok(list_value(items))
    }
}

#[async_trait]
impl Action for HtmlLinks {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "html.links",
            "HTML 提取链接",
            "取页面里的链接（默认 a[href]），可补成绝对 URL 并过滤锚点/脚本",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("html", SchemaType::Str, true),
            ParamSchema::new("selector", SchemaType::Str, false)
                .with_default("a[href]")
                .with_description("挑哪些元素；`attr` 默认取 href"),
            ParamSchema::new("attr", SchemaType::Str, false).with_default("href"),
            ParamSchema::new("base", SchemaType::Str, false).with_description("当前页面的 URL"),
            ParamSchema::new("absolute", SchemaType::Bool, false)
                .with_description("给了 base 就默认绝对化；显式 false 保留原样"),
            ParamSchema::new("junk", SchemaType::Bool, false)
                .with_default(false)
                .with_description("保留 # 锚点、javascript:、data: 这类链接"),
            ParamSchema::new("unique", SchemaType::Bool, false)
                .with_default(true)
                .with_description("去重，保持首次出现的顺序"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let html = require_str(map, "html")?;
        let css = opt_str(map, "selector").unwrap_or_else(|| "a[href]".into());
        let attr = opt_str(map, "attr").unwrap_or_else(|| "href".into());
        let keep_junk = opt_bool(map, "junk", false);
        let unique = opt_bool(map, "unique", true);
        let base = find_base(map)?;

        let (doc, selector) = parse(&html, &css)?;
        let mut items: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for el in doc.select(&selector) {
            let Some(raw) = extract(&el, Some(&attr), true) else {
                continue;
            };
            if !keep_junk && is_junk_link(&raw) {
                continue;
            }
            let url = match &base {
                Some(base) => join(base, &raw),
                None => raw,
            };
            // 去重走哈希表：几千条链接的页面上 `Vec::contains` 就是 O(n²)。
            if unique && !seen.insert(url.clone()) {
                continue;
            }
            items.push(url);
        }

        Ok(list_value(items))
    }
}

#[async_trait]
impl Action for HtmlText {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "html.text",
            "HTML 取正文",
            "去掉标签与脚本，把匹配元素的文本拼成纯文本",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("html", SchemaType::Str, true),
            ParamSchema::new("selector", SchemaType::Str, false)
                .with_default("body")
                .with_description("取哪块的正文；默认整页"),
            ParamSchema::new("separator", SchemaType::Str, false)
                .with_default("\n")
                .with_description("多个匹配元素之间的连接符"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let html = require_str(map, "html")?;
        let css = opt_str(map, "selector").unwrap_or_else(|| "body".into());
        let separator = opt_str(map, "separator").unwrap_or_else(|| "\n".into());

        let (doc, selector) = parse(&html, &css)?;
        let parts: Vec<String> = doc
            .select(&selector)
            .map(|el| visible_text(&el).trim().to_string())
            .filter(|part| !part.is_empty())
            .collect();
        Ok(Value::Str(parts.join(&separator)))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(HtmlSelect));
    registry.register(Arc::new(HtmlLinks));
    registry.register(Arc::new(HtmlText));
}

#[cfg(test)]
mod tests {
    use super::*;

    // 定界符用 `r##"…"##`：页面里有 `href="#top"`，`"#` 会提前结束 `r#"…"#`。
    const PAGE: &str = r##"<!doctype html>
<html><head><title>T &amp; T</title></head>
<body>
  <h1 class="title">Hello <b>world</b></h1>
  <ul id="list">
    <li class="item" data-id="7">第一项</li>
    <li class="item" data-id="8">第二项</li>
  </ul>
  <a href="/a/b">相对</a>
  <a href="https://example.com/x">绝对</a>
  <a href="#top">锚点</a>
  <a href="javascript:void(0)">脚本</a>
  <a href="/a/b">重复</a>
</body></html>"##;

    fn map(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    }

    fn field<'a>(out: &'a Value, key: &str) -> &'a Value {
        out.as_map().unwrap().get(key).unwrap()
    }

    fn urls_of(out: &Value) -> Vec<&str> {
        field(out, "items")
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect()
    }

    async fn run(action: &dyn Action, params: Value) -> Value {
        let mut ctx = ExecutionContext::default();
        action.execute(params, &mut ctx).await.unwrap()
    }

    #[tokio::test]
    async fn select_takes_text_and_attributes() {
        let out = run(
            &HtmlSelect,
            map(&[
                ("html", Value::Str(PAGE.into())),
                ("selector", Value::Str(".item".into())),
            ]),
        )
        .await;
        assert_eq!(field(&out, "count"), &Value::Int(2));
        let items = field(&out, "items").as_array().unwrap();
        assert_eq!(items[0].as_str(), Some("第一项"));
        assert_eq!(field(&out, "value").as_str(), Some("第一项"));

        let out = run(
            &HtmlSelect,
            map(&[
                ("html", Value::Str(PAGE.into())),
                ("selector", Value::Str(".item".into())),
                ("attr", Value::Str("data-id".into())),
            ]),
        )
        .await;
        let items = field(&out, "items").as_array().unwrap();
        assert_eq!(items[1].as_str(), Some("8"));
    }

    /// 实体由分词器解码，内联标签不该被空格撑开。
    #[tokio::test]
    async fn select_joins_inline_text_and_decodes_entities() {
        let out = run(
            &HtmlSelect,
            map(&[
                ("html", Value::Str(PAGE.into())),
                ("selector", Value::Str("h1".into())),
            ]),
        )
        .await;
        assert_eq!(field(&out, "value").as_str(), Some("Hello world"));

        let out = run(
            &HtmlSelect,
            map(&[
                ("html", Value::Str(PAGE.into())),
                ("selector", Value::Str("title".into())),
            ]),
        )
        .await;
        assert_eq!(field(&out, "value").as_str(), Some("T & T"));
    }

    #[tokio::test]
    async fn select_all_false_and_limit() {
        let out = run(
            &HtmlSelect,
            map(&[
                ("html", Value::Str(PAGE.into())),
                ("selector", Value::Str("li".into())),
                ("all", Value::Bool(false)),
            ]),
        )
        .await;
        assert_eq!(field(&out, "count"), &Value::Int(1));
        assert_eq!(field(&out, "value").as_str(), Some("第一项"));
    }

    #[tokio::test]
    async fn links_are_absolutized_deduped_and_filtered() {
        let out = run(
            &HtmlLinks,
            map(&[
                ("html", Value::Str(PAGE.into())),
                (
                    "base",
                    Value::Str("https://example.com/root/index.html".into()),
                ),
            ]),
        )
        .await;
        assert_eq!(
            urls_of(&out),
            vec!["https://example.com/a/b", "https://example.com/x"]
        );
    }

    /// 没给 `base` 就保留原样：相对链接也是合法结果，不必报错；锚点与脚本仍过滤。
    #[tokio::test]
    async fn links_without_base_keep_relative_form() {
        let out = run(&HtmlLinks, map(&[("html", Value::Str(PAGE.into()))])).await;
        assert_eq!(urls_of(&out), vec!["/a/b", "https://example.com/x"]);
    }

    /// 给了 `base` 也能显式关掉绝对化。
    #[tokio::test]
    async fn links_can_keep_relative_form_with_base() {
        let out = run(
            &HtmlLinks,
            map(&[
                ("html", Value::Str(PAGE.into())),
                (
                    "base",
                    Value::Str("https://example.com/root/index.html".into()),
                ),
                ("absolute", Value::Bool(false)),
            ]),
        )
        .await;
        assert_eq!(urls_of(&out), vec!["/a/b", "https://example.com/x"]);
    }

    /// 要绝对化却没给 base：明确报错，而不是悄悄回退成相对链接。
    #[tokio::test]
    async fn explicit_absolute_without_base_is_an_error() {
        let mut ctx = ExecutionContext::default();
        let err = HtmlLinks
            .execute(
                map(&[
                    ("html", Value::Str(PAGE.into())),
                    ("absolute", Value::Bool(true)),
                ]),
                &mut ctx,
            )
            .await
            .expect_err("缺 base");
        assert!(err.to_string().contains("base"), "{err}");
    }

    #[tokio::test]
    async fn text_strips_markup() {
        let out = run(
            &HtmlText,
            map(&[
                ("html", Value::Str(PAGE.into())),
                ("selector", Value::Str("ul".into())),
            ]),
        )
        .await;
        let text = out.as_str().unwrap();
        assert!(text.contains("第一项"), "{text}");
        assert!(text.contains("第二项"), "{text}");
        assert!(!text.contains('<'), "{text}");
    }

    /// 内联代码不是正文。
    ///
    /// `ElementRef::text()` 会把 `script` / `style` 的文字一起收进来，之前正文里
    /// 就混进了整段 JS——这条函数把 `visible_text` 的行为钉住。
    const PAGE_WITH_CODE: &str = r##"<html><body>
      <h1>Title</h1>
      <script>var secret = "JS_MARKER";</script>
      <style>.a{color:red}</style>
      <p>Body</p>
    </body></html>"##;

    #[tokio::test]
    async fn text_and_select_skip_script_and_style() {
        let out = run(
            &HtmlText,
            map(&[("html", Value::Str(PAGE_WITH_CODE.into()))]),
        )
        .await;
        let text = out.as_str().unwrap();
        assert!(text.contains("Title") && text.contains("Body"), "{text}");
        assert!(!text.contains("JS_MARKER"), "{text}");
        assert!(!text.contains("color:red"), "{text}");

        // 不带 `attr` 的 `html.select` 走同一条正文提取。
        let out = run(
            &HtmlSelect,
            map(&[
                ("html", Value::Str(PAGE_WITH_CODE.into())),
                ("selector", Value::Str("body".into())),
            ]),
        )
        .await;
        let value = field(&out, "value").as_str().unwrap();
        assert!(!value.contains("JS_MARKER"), "{value}");
    }

    /// 选择器写错要报参数错，而不是静默返回空。
    #[tokio::test]
    async fn bad_selector_is_an_error() {
        let mut ctx = ExecutionContext::default();
        let err = HtmlSelect
            .execute(
                map(&[
                    ("html", Value::Str(PAGE.into())),
                    ("selector", Value::Str("li[[".into())),
                ]),
                &mut ctx,
            )
            .await
            .expect_err("选择器非法");
        assert!(err.to_string().contains("选择器"), "{err}");
    }
}
