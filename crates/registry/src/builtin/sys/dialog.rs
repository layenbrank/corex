//! 原生对话框：MessageBox 提示 / 输入、打开与保存文件、单选选择（Windows）。
//!
//! 用户取消一律编码进返回值（`ok: false`）而不是当错误抛——取消是正常操作，
//! 指令可以据此决定要不要继续；只有对话框本身失败（如 `CommDlgExtendedError` 非 0）
//! 才是错误。

use crate::ActionRegistry;
use crate::builtin::util::{opt_str, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// 没给 `filter` 时的兜底：所有文件。
const DEFAULT_FILTER: &str = "所有文件|*.*";
/// 对话框默认标题。
const DEFAULT_TITLE: &str = "corex";

pub struct DialogAlert;
pub struct DialogConfirm;
pub struct DialogPrompt;
pub struct DialogOpen;
pub struct DialogSave;
pub struct DialogChoice;

#[async_trait]
impl Action for DialogAlert {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::UI
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dialog.alert",
            "提示对话框",
            "模态提示框（确定）",
            Bucket::Ui,
        )
        .with_params(vec![
            ParamSchema::new("message", SchemaType::Str, true),
            ParamSchema::new("title", SchemaType::Str, false).with_default("corex"),
        ])
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_alert(params).await
    }
}

#[async_trait]
impl Action for DialogConfirm {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::UI
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("dialog.confirm", "确认对话框", "是/否确认框", Bucket::Ui).with_params(
            vec![
                ParamSchema::new("message", SchemaType::Str, true),
                ParamSchema::new("title", SchemaType::Str, false).with_default("corex"),
            ],
        )
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_confirm(params).await
    }
}

#[async_trait]
impl Action for DialogPrompt {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::UI
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("dialog.prompt", "输入对话框", "简单文本输入框", Bucket::Ui).with_params(
            vec![
                ParamSchema::new("message", SchemaType::Str, true),
                ParamSchema::new("title", SchemaType::Str, false).with_default("corex"),
                ParamSchema::new("default", SchemaType::Str, false),
            ],
        )
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_prompt(params).await
    }
}

#[async_trait]
impl Action for DialogOpen {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::UI
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dialog.open",
            "选择文件",
            "系统「打开文件」对话框；取消时 ok=false、path 为空",
            Bucket::Ui,
        )
        .with_params(vec![
            ParamSchema::new("title", SchemaType::Str, false).with_default(DEFAULT_TITLE),
            ParamSchema::new("filter", SchemaType::Str, false)
                .with_default(DEFAULT_FILTER)
                .with_description("“名称|通配”成对，用 | 分隔，如 文本|*.txt|所有文件|*.*"),
            ParamSchema::new("initial_dir", SchemaType::Str, false)
                .with_description("起始目录（不存在时系统自行回落）"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_pick(&params, false, "dialog.open").await
    }
}

#[async_trait]
impl Action for DialogSave {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::UI
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dialog.save",
            "保存文件",
            "系统「另存为」对话框；取消时 ok=false、path 为空",
            Bucket::Ui,
        )
        .with_params(vec![
            ParamSchema::new("title", SchemaType::Str, false).with_default(DEFAULT_TITLE),
            ParamSchema::new("filter", SchemaType::Str, false)
                .with_default(DEFAULT_FILTER)
                .with_description("“名称|通配”成对，用 | 分隔，如 文本|*.txt|所有文件|*.*"),
            ParamSchema::new("initial_dir", SchemaType::Str, false)
                .with_description("起始目录（不存在时系统自行回落）"),
            ParamSchema::new("default_name", SchemaType::Str, false)
                .with_description("预填的文件名"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_pick(&params, true, "dialog.save").await
    }
}

#[async_trait]
impl Action for DialogChoice {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::UI
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dialog.choice",
            "单选对话框",
            "把 options 渲染成一列按钮，返回选中项的下标与原始值；取消时 ok=false",
            Bucket::Ui,
        )
        .with_params(vec![
            ParamSchema::new("prompt", SchemaType::Str, true).with_description("要问的问题"),
            ParamSchema::new("title", SchemaType::Str, false).with_default(DEFAULT_TITLE),
            ParamSchema::new("options", SchemaType::Array, true)
                .with_description("选项数组（标量；字符串项直接当按钮文字，数字/布尔取字面量）"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_choice(params).await
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(DialogAlert));
    registry.register(Arc::new(DialogConfirm));
    registry.register(Arc::new(DialogOpen));
    registry.register(Arc::new(DialogPrompt));
    registry.register(Arc::new(DialogSave));
    registry.register(Arc::new(DialogChoice));
}

/// 标题统一在这里取默认值：三个 `dialog.*` 都用它，改默认值只改一处。
fn require_title(map: &BTreeMap<String, Value>) -> String {
    opt_str(map, "title").unwrap_or_else(|| DEFAULT_TITLE.to_string())
}

fn require_message(params: &Value) -> Result<(String, String), ActionError> {
    let map = require_map(params)?;
    Ok((require_str(map, "message")?, require_title(map)))
}

async fn dialog_alert(params: Value) -> Result<Value, ActionError> {
    let (message, title) = require_message(&params)?;
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || win::message_box(&title, &message, false))
            .await
            .map_err(|e| ActionError::execution(format!("dialog.alert 失败: {e}")))?
            .map(|_| Value::Bool(true))
    }
    #[cfg(not(windows))]
    {
        let _ = (message, title);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

async fn dialog_confirm(params: Value) -> Result<Value, ActionError> {
    let (message, title) = require_message(&params)?;
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || win::message_box(&title, &message, true))
            .await
            .map_err(|e| ActionError::execution(format!("dialog.confirm 失败: {e}")))?
            .map(Value::Bool)
    }
    #[cfg(not(windows))]
    {
        let _ = (message, title);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

async fn dialog_prompt(params: Value) -> Result<Value, ActionError> {
    let (message, title) = require_message(&params)?;
    let default = opt_str(require_map(&params)?, "default").unwrap_or_default();
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || win::prompt_box(&title, &message, &default))
            .await
            .map_err(|e| ActionError::execution(format!("dialog.prompt 失败: {e}")))?
    }
    #[cfg(not(windows))]
    {
        let _ = (message, title, default);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

/// 打开/保存文件都走这里：解析参数 → 弹框 → 把「取消」编码进结果。
async fn dialog_pick(params: &Value, save: bool, id: &str) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        let query = win::PathQuery::from_params(params, save)?;
        let picked = tokio::task::spawn_blocking(move || win::pick_path(&query))
            .await
            .map_err(|e| ActionError::execution(format!("{id} 失败: {e}")))??;
        Ok(win::file_result(picked))
    }
    #[cfg(not(windows))]
    {
        let _ = (params, save, id);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

async fn dialog_choice(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let prompt = require_str(map, "prompt")?;
    let title = require_title(map);
    let options = choice_options(map)?;
    #[cfg(windows)]
    {
        let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();
        let picked = tokio::task::spawn_blocking(move || win::choice_box(&title, &prompt, &labels))
            .await
            .map_err(|e| ActionError::execution(format!("dialog.choice 失败: {e}")))??;
        Ok(win::choice_result(picked, &options))
    }
    #[cfg(not(windows))]
    {
        let _ = (prompt, title, options);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

/// `(按钮文字, 原始值)`：文字给对话框画按钮，原始值随结果回传，
/// 于是指令可以写 `options: ["覆盖", "跳过"]` 这种人类可读的写法。
fn choice_options(map: &BTreeMap<String, Value>) -> Result<Vec<(String, Value)>, ActionError> {
    let items = match map.get("options") {
        Some(Value::Array(items)) => items,
        _ => return Err(ActionError::InvalidParams("options 必须是数组".into())),
    };
    if items.is_empty() {
        return Err(ActionError::InvalidParams("options 不能为空".into()));
    }
    items
        .iter()
        .map(|item| match item {
            Value::Map(_) | Value::Array(_) | Value::Bytes(_) => Err(ActionError::InvalidParams(
                "options 只接受标量（字符串 / 数字 / 布尔）".into(),
            )),
            scalar => Ok((scalar.to_string(), scalar.clone())),
        })
        .collect()
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Controls::BST_CHECKED;
    use windows::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
        OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_GETCHECK, BM_SETCHECK, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, DLGTEMPLATE,
        DS_CENTER, DS_MODALFRAME, DialogBoxIndirectParamW, EndDialog, GetDlgItem,
        GetForegroundWindow, IDCANCEL, IDOK, IDYES, MB_OK, MB_YESNO, MESSAGEBOX_RESULT,
        MESSAGEBOX_STYLE, MessageBoxW, SendMessageW, WM_CLOSE, WM_COMMAND, WM_INITDIALOG,
        WS_CAPTION, WS_CHILD, WS_GROUP, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    };
    use windows::core::{PCWSTR, PWSTR};

    /// 用户选的路径最长 32k（`nMaxFile` 的上限就是这个量级）。
    const PATH_CAPACITY: usize = 32_768;

    /// 单选对话框的尺寸都按「对话框单位」给：系统按当前字体换算成像素，高 DPI 不用自己缩放。
    const CHOICE_WIDTH: i16 = 230;
    /// 一行单选按钮的高度。
    const RADIO_ROW: i16 = 12;
    /// 输入提示的文本可能很长，按两行留高度，宁可多留空也不要被裁掉。
    const PROMPT_HEIGHT: i16 = 18;
    const BUTTON_HEIGHT: i16 = 14;
    const BUTTON_WIDTH: i16 = 48;
    /// 控件到对话框边框的间隙。
    const GAP: i16 = 7;
    /// 同组控件之间的间隙。
    const SEPARATOR: i16 = 6;

    /// `WS_POPUP|WS_CAPTION|WS_SYSMENU` + 厚边框 + 居中：模态对话框的标准长相。
    const CHOICE_STYLE: u32 =
        WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | DS_MODALFRAME as u32 | DS_CENTER as u32;

    /// 单选按钮的控件 ID 从这里往上排。
    const RADIO_BASE: i32 = 1000;
    /// 静态提示不需要 ID，按约定填 `0xFFFF`。
    const NO_ID: u16 = u16::MAX;
    /// `IDOK` / `IDCANCEL`：Win32 给确定/取消按钮定的 ID。
    const DIALOG_OK: i32 = IDOK.0;
    const DIALOG_CANCEL: i32 = IDCANCEL.0;
    /// 内置控件类在模板里写作 `0xFFFF + 序数`，省掉类名字符串。
    const BUTTON_CLASS: u16 = 0x0080;
    const STATIC_CLASS: u16 = 0x0082;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// 对话框属主取当前前台窗口：没有属主的话，弹窗可能被别的窗口盖住，
    /// 看起来就像指令卡死了。取不到就退回无属主。
    fn foreground() -> HWND {
        unsafe { GetForegroundWindow() }
    }

    pub fn message_box(title: &str, message: &str, yes_no: bool) -> Result<bool, ActionError> {
        let t = wide(title);
        let m = wide(message);
        let style: MESSAGEBOX_STYLE = if yes_no { MB_YESNO } else { MB_OK };
        let r: MESSAGEBOX_RESULT =
            unsafe { MessageBoxW(None, PCWSTR(m.as_ptr()), PCWSTR(t.as_ptr()), style) };
        if yes_no { Ok(r == IDYES) } else { Ok(true) }
    }

    /// 最简输入提示：显示消息与默认文本，用户点“是”接受默认值、点“否”取消。
    /// 为避免体积膨胀不做完整编辑框——点“是”就返回默认值。
    /// 需要更丰富的输入时，在指令里配合剪贴板动作。
    pub fn prompt_box(title: &str, message: &str, default: &str) -> Result<Value, ActionError> {
        let body = if default.is_empty() {
            format!("{message}\n\n（确认后返回空文本；可先把内容放入剪贴板）")
        } else {
            format!("{message}\n\n默认值: {default}\n（是=使用默认值，否=取消）")
        };
        let ok = message_box(title, &body, true)?;
        let mut out = BTreeMap::new();
        out.insert("ok".into(), Value::Bool(ok));
        out.insert(
            "text".into(),
            Value::Str(if ok {
                default.to_string()
            } else {
                String::new()
            }),
        );
        Ok(Value::Map(out))
    }

    /// 文件对话框的一次询问：`dialog.open` 与 `dialog.save` 只差 flags 与预填文件名。
    pub struct PathQuery {
        pub title: String,
        pub filters: Vec<(String, String)>,
        pub initial_dir: Option<String>,
        pub name: Option<String>,
        pub save: bool,
    }

    impl PathQuery {
        pub fn from_params(params: &Value, save: bool) -> Result<Self, ActionError> {
            let map = require_map(params)?;
            let filter = opt_str(map, "filter").unwrap_or_else(|| DEFAULT_FILTER.to_string());
            Ok(Self {
                title: require_title(map),
                filters: filter_pairs(&filter)?,
                initial_dir: opt_str(map, "initial_dir"),
                // `default_name` 只有保存侧收：打开侧预填文件名没有意义。
                name: if save {
                    opt_str(map, "default_name")
                } else {
                    None
                },
                save,
            })
        }
    }

    /// 把 COM 的过滤串切成 `(名称, 通配)` 对：`文本|*.txt|所有文件|*.*`。
    /// 名称与通配必须成对，否则用户看到的是错位的一串。
    pub fn filter_pairs(spec: &str) -> Result<Vec<(String, String)>, ActionError> {
        let parts: Vec<&str> = spec.split('|').map(str::trim).collect();
        if !parts.len().is_multiple_of(2) || parts.iter().any(|part| part.is_empty()) {
            return Err(ActionError::InvalidParams(format!(
                "filter 必须是“名称|通配”成对、用 | 分隔: {spec}"
            )));
        }
        Ok(parts
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| (pair[0].to_string(), pair[1].to_string()))
            .collect())
    }

    /// `filter` 参数要的是双 NUL 结尾的 `名称\0通配\0…\0` 串。
    fn filter_wide(filters: &[(String, String)]) -> Vec<u16> {
        let mut spec = String::new();
        for (label, pattern) in filters {
            spec.push_str(label);
            spec.push('\0');
            spec.push_str(pattern);
            spec.push('\0');
        }
        spec.push('\0');
        spec.encode_utf16().collect()
    }

    /// 弹出打开/另存为对话框。返回 `None` 表示用户取消（此时 `CommDlgExtendedError` 为 0），
    /// 其它非 0 码才是真的失败。
    pub fn pick_path(query: &PathQuery) -> Result<Option<String>, ActionError> {
        let filter = filter_wide(&query.filters);
        let title = wide(&query.title);
        let dir = query.initial_dir.as_deref().map(wide);
        let mut file = vec![0u16; PATH_CAPACITY];
        if let Some(name) = &query.name {
            let name = wide(name);
            if name.len() > file.len() {
                return Err(ActionError::InvalidParams("default_name 过长".into()));
            }
            file[..name.len()].copy_from_slice(&name);
        }
        // `OFN_NOCHANGEDIR`：常驻 daemon 里不能让对话框把进程工作目录改掉。
        let flags = if query.save {
            OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        } else {
            OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        };
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: foreground(),
            lpstrFilter: PCWSTR(filter.as_ptr()),
            nFilterIndex: 1,
            lpstrFile: PWSTR(file.as_mut_ptr()),
            nMaxFile: file.len() as u32,
            lpstrInitialDir: dir
                .as_ref()
                .map_or(PCWSTR::null(), |dir| PCWSTR(dir.as_ptr())),
            lpstrTitle: PCWSTR(title.as_ptr()),
            Flags: flags,
            ..Default::default()
        };
        let picked = unsafe {
            if query.save {
                GetSaveFileNameW(&mut ofn)
            } else {
                GetOpenFileNameW(&mut ofn)
            }
        };
        if picked.as_bool() {
            return Ok(Some(read_wide(&file)));
        }
        let code = unsafe { CommDlgExtendedError() }.0;
        if code == 0 {
            Ok(None)
        } else {
            Err(ActionError::execution(format!(
                "文件对话框失败 code=0x{code:04X}"
            )))
        }
    }

    /// 单选对话框：`labels` 画成一列单选按钮，返回选中项的下标；取消/关闭返回 `None`。
    pub fn choice_box(
        title: &str,
        prompt: &str,
        labels: &[String],
    ) -> Result<Option<usize>, ActionError> {
        let template = aligned_u32(&choice_template(title, prompt, labels));
        // 返回值就是对话框过程通过 `EndDialog` 传出的结果：
        // -1 表示创建失败，0 表示取消，≥1 是选中下标 + 1。
        let result = unsafe {
            DialogBoxIndirectParamW(
                None,
                template.as_ptr().cast::<DLGTEMPLATE>(),
                Some(foreground()),
                Some(choice_proc),
                LPARAM(0),
            )
        };
        match result {
            -1 => Err(ActionError::execution("单选对话框创建失败")),
            picked if picked >= 1 => Ok(Some(picked as usize - 1)),
            _ => Ok(None),
        }
    }

    /// 手工拼一份 `DLGTEMPLATE`。
    ///
    /// 为什么不用 `TaskDialogIndirect`：它只在 comctl32 的 SxS v6 版本里导出，
    /// 而进程没有引用清单时静态链上去会让**整个进程在加载期**失败
    /// （`STATUS_ENTRYPOINT_NOT_FOUND`），连 `corex --help` 都跑不起来。
    /// 手工拼模板只用 user32，没有这个坑。
    pub fn choice_template(title: &str, prompt: &str, labels: &[String]) -> Vec<u8> {
        let rows = labels.len() as i16;
        let height =
            2 * GAP + PROMPT_HEIGHT + SEPARATOR + RADIO_ROW * rows + SEPARATOR + BUTTON_HEIGHT;
        let mut buf = Vec::with_capacity(512);
        push_u32(&mut buf, CHOICE_STYLE);
        push_u32(&mut buf, 0); // dwExtendedStyle
        push_u16(&mut buf, (labels.len() + 3) as u16); // cdit：提示 + 每个选项 + 确定/取消
        // x/y/cx/cy：位置交给 `DS_CENTER`，尺寸自己定。
        for value in [0i16, 0, CHOICE_WIDTH, height] {
            buf.extend_from_slice(&value.to_le_bytes());
        }
        // 菜单与窗口类都用默认值，紧接着是标题。
        push_u16(&mut buf, 0);
        push_u16(&mut buf, 0);
        push_text(&mut buf, title);

        let inner = CHOICE_WIDTH - 2 * GAP;
        let mut top = GAP;
        push_item(
            &mut buf,
            WS_CHILD.0 | WS_VISIBLE.0, // 静态文本默认左对齐且自动换行（`SS_LEFT` 就是 0）
            [GAP, top, inner, PROMPT_HEIGHT],
            NO_ID,
            STATIC_CLASS,
            prompt,
        );
        top += PROMPT_HEIGHT + SEPARATOR;
        for (index, label) in labels.iter().enumerate() {
            // 第一项带 `WS_GROUP|WS_TABSTOP`：user32 靠 `WS_GROUP` 把这一串单选按钮认成一组，
            // 组内用方向键切换，Tab 直接跳到按钮区。
            let group = if index == 0 {
                WS_GROUP.0 | WS_TABSTOP.0
            } else {
                0
            };
            let style = WS_CHILD.0 | WS_VISIBLE.0 | BS_AUTORADIOBUTTON as u32 | group;
            push_item(
                &mut buf,
                style,
                [GAP + 3, top, inner - 3, RADIO_ROW],
                RADIO_BASE as u16 + index as u16,
                BUTTON_CLASS,
                label,
            );
            top += RADIO_ROW;
        }
        // 确定/取消靠右下角摆。
        let button_top = height - GAP - BUTTON_HEIGHT;
        let cancel_x = CHOICE_WIDTH - GAP - BUTTON_WIDTH;
        let ok_x = cancel_x - SEPARATOR - BUTTON_WIDTH;
        let (ok, cancel) = (
            WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32,
            WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
        );
        push_item(
            &mut buf,
            ok,
            [ok_x, button_top, BUTTON_WIDTH, BUTTON_HEIGHT],
            DIALOG_OK as u16,
            BUTTON_CLASS,
            "确定",
        );
        push_item(
            &mut buf,
            cancel,
            [cancel_x, button_top, BUTTON_WIDTH, BUTTON_HEIGHT],
            DIALOG_CANCEL as u16,
            BUTTON_CLASS,
            "取消",
        );
        buf
    }

    /// `DLGTEMPLATE` 要求指针 4 字节对齐，而 `Vec<u8>` 只保证 1 字节，
    /// 于是把拼好的字节流按小端重排进 `u32` 数组再交给系统。
    pub fn aligned_u32(bytes: &[u8]) -> Vec<u32> {
        let mut words = vec![0u32; bytes.len().div_ceil(4)];
        for (index, byte) in bytes.iter().enumerate() {
            words[index / 4] |= u32::from(*byte) << (8 * (index % 4));
        }
        words
    }

    /// 对话框过程：弹出时预选第一项，点确定时把选中下标 +1 回传
    /// （0 留给取消，-1 是系统约定的创建失败）。
    unsafe extern "system" fn choice_proc(
        hdlg: HWND,
        msg: u32,
        wparam: WPARAM,
        _lparam: LPARAM,
    ) -> isize {
        match msg {
            WM_INITDIALOG => {
                check_radio(hdlg, 0);
                // 返回 0：让系统把焦点交给第一个带 `WS_TABSTOP` 的控件。
                0
            }
            WM_COMMAND => {
                let result = match (wparam.0 & 0xFFFF) as i32 {
                    DIALOG_OK => picked_index(hdlg).map_or(0, |index| index as isize + 1),
                    DIALOG_CANCEL => 0,
                    // 其余（比如单选按钮被点）不处理，交回系统。
                    _ => return 0,
                };
                let _ = unsafe { EndDialog(hdlg, result) };
                1
            }
            WM_CLOSE => {
                let _ = unsafe { EndDialog(hdlg, 0) };
                1
            }
            _ => 0,
        }
    }

    /// 预选第 `index` 项：用户直接回车就能确认，不用先点一下。
    fn check_radio(hdlg: HWND, index: usize) {
        if let Ok(item) = unsafe { GetDlgItem(Some(hdlg), RADIO_BASE + index as i32) } {
            unsafe {
                SendMessageW(
                    item,
                    BM_SETCHECK,
                    Some(WPARAM(BST_CHECKED.0 as usize)),
                    None,
                )
            };
        }
    }

    /// 顺着控件 ID 找选中的单选按钮：不存任何状态，取消后重开也不会串上次的选择。
    fn picked_index(hdlg: HWND) -> Option<usize> {
        for index in 0.. {
            let item = unsafe { GetDlgItem(Some(hdlg), RADIO_BASE + index as i32) }.ok()?;
            if unsafe { SendMessageW(item, BM_GETCHECK, None, None) }.0 == BST_CHECKED.0 as isize {
                return Some(index);
            }
        }
        None
    }

    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(buf: &mut Vec<u8>, value: u32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_text(buf: &mut Vec<u8>, text: &str) {
        for unit in text.encode_utf16() {
            push_u16(buf, unit);
        }
        push_u16(buf, 0);
    }

    /// 拼一个子控件模板。`rect` 按 `x/y/cx/cy` 给；开头补的填充字节是格式要求，
    /// 每个 `DLGITEMTEMPLATE` 必须 4 字节对齐。
    fn push_item(buf: &mut Vec<u8>, style: u32, rect: [i16; 4], id: u16, class: u16, text: &str) {
        buf.resize(buf.len().next_multiple_of(4), 0);
        push_u32(buf, style);
        push_u32(buf, 0); // dwExtendedStyle
        for value in rect {
            buf.extend_from_slice(&value.to_le_bytes());
        }
        push_u16(buf, id);
        push_u16(buf, 0xFFFF); // 类用序数表示
        push_u16(buf, class);
        push_text(buf, text);
        push_u16(buf, 0); // 创建数据长度为 0
    }

    /// 取消时 `path` 是空路径（而不是缺字段），指令里引用它就不会炸。
    pub fn file_result(picked: Option<String>) -> Value {
        let path = picked.map(PathBuf::from).unwrap_or_default();
        let mut out = BTreeMap::new();
        out.insert("ok".into(), Value::Bool(!path.as_os_str().is_empty()));
        out.insert("path".into(), Value::File(path));
        Value::Map(out)
    }

    /// 取消时 `index` 是 -1、`value` 是 null：三个字段恒在，指令不必分支判类型。
    pub fn choice_result(index: Option<usize>, options: &[(String, Value)]) -> Value {
        let mut out = BTreeMap::new();
        out.insert("ok".into(), Value::Bool(index.is_some()));
        out.insert("index".into(), Value::Int(index.map_or(-1, |i| i as i64)));
        out.insert(
            "value".into(),
            index
                .and_then(|i| options.get(i).map(|(_, value)| value.clone()))
                .unwrap_or(Value::Null),
        );
        Value::Map(out)
    }

    /// 取对话框缓冲区里第一个 NUL 之前的部分。
    fn read_wide(buf: &[u16]) -> String {
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn text(value: &str) -> Value {
        Value::Str(value.to_string())
    }

    fn params(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
    }

    #[test]
    fn filter_pairs_requires_name_and_pattern_pairs() {
        let pairs = win::filter_pairs("文本|*.txt| 所有文件 |*.*").unwrap();
        assert_eq!(
            pairs,
            vec![
                ("文本".to_string(), "*.txt".to_string()),
                ("所有文件".to_string(), "*.*".to_string()),
            ]
        );

        for bad in ["文本|*.txt|所有文件", "", "文本||*.*", "只有名称"] {
            let err = win::filter_pairs(bad).unwrap_err();
            assert!(matches!(err, ActionError::InvalidParams(_)), "got: {err}");
        }
    }

    #[test]
    fn pick_query_reads_only_what_each_side_needs() {
        let open = win::PathQuery::from_params(
            &params(&[
                ("title", text("挑一个")),
                ("default_name", text("草稿.txt")),
                ("initial_dir", text(r"C:\")),
            ]),
            false,
        )
        .unwrap();
        assert_eq!(open.title, "挑一个");
        assert_eq!(open.filters, win::filter_pairs(DEFAULT_FILTER).unwrap());
        assert_eq!(open.initial_dir.as_deref(), Some(r"C:\"));
        assert!(open.name.is_none(), "打开侧不该预填文件名");

        let save = win::PathQuery::from_params(
            &params(&[
                ("filter", text("文本|*.txt")),
                ("default_name", text("草稿.txt")),
            ]),
            true,
        )
        .unwrap();
        assert_eq!(
            save.filters,
            vec![("文本".to_string(), "*.txt".to_string())]
        );
        assert_eq!(save.name.as_deref(), Some("草稿.txt"));
        assert_eq!(save.title, DEFAULT_TITLE);
    }

    #[test]
    fn file_result_keeps_path_a_string_even_when_cancelled() {
        let picked = win::file_result(Some(r"C:\tmp\a.txt".to_string()));
        let map = picked.as_map().unwrap();
        assert_eq!(map.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            map.get("path").and_then(Value::as_str),
            Some(r"C:\tmp\a.txt")
        );

        let cancelled = win::file_result(None);
        let map = cancelled.as_map().unwrap();
        assert_eq!(map.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(map.get("path").and_then(Value::as_str), Some(""));
    }

    #[test]
    fn choice_result_maps_index_back_to_the_original_value() {
        let options = vec![
            ("覆盖".to_string(), text("overwrite")),
            ("跳过".to_string(), Value::Int(2)),
        ];
        let picked = win::choice_result(Some(1), &options);
        let map = picked.as_map().unwrap();
        assert_eq!(map.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(map.get("index").and_then(Value::as_i64), Some(1));
        assert_eq!(map.get("value").and_then(Value::as_i64), Some(2));

        let cancelled = win::choice_result(None, &options);
        let map = cancelled.as_map().unwrap();
        assert_eq!(map.get("ok").and_then(Value::as_bool), Some(false));
        assert_eq!(map.get("index").and_then(Value::as_i64), Some(-1));
        assert!(matches!(map.get("value"), Some(Value::Null)));
    }

    #[test]
    fn choice_options_accept_scalars_only() {
        let options = choice_options(&options_map(vec![
            text("a"),
            Value::Int(1),
            Value::Bool(true),
        ]))
        .unwrap();
        assert_eq!(
            options,
            vec![
                ("a".to_string(), text("a")),
                ("1".to_string(), Value::Int(1)),
                ("true".to_string(), Value::Bool(true)),
            ]
        );

        let object = choice_options(&options_map(vec![Value::Map(BTreeMap::new())]));
        assert!(matches!(object, Err(ActionError::InvalidParams(_))));

        let empty = choice_options(&options_map(vec![]));
        assert!(matches!(empty, Err(ActionError::InvalidParams(_))));

        let missing = choice_options(&BTreeMap::new());
        assert!(matches!(missing, Err(ActionError::InvalidParams(_))));
    }

    fn options_map(items: Vec<Value>) -> BTreeMap<String, Value> {
        let mut map = BTreeMap::new();
        map.insert("options".to_string(), Value::Array(items));
        map
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Item {
        class: u16,
        id: u16,
        rect: [i16; 4],
    }

    #[test]
    fn choice_template_lays_out_prompt_options_and_buttons() {
        let labels = labels(&["覆盖", "跳过"]);
        let bytes = win::choice_template("选择处理方式", "已存在同名文件，怎么处理？", &labels);
        assert_eq!(read_u16(&bytes, 8), 5, "cdit = 提示 + 2 个选项 + 确定/取消");
        let width = read_u16(&bytes, 14) as i16;

        let items = template_items(&bytes);
        // id：提示不挂 ID，两个选项接在 1000 之后，最后是系统约定的确定/取消。
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![0xFFFF, 1000, 1001, 1, 2]
        );
        // 类：一个静态文本，其余是按钮（内置控件类用序数表示）。
        assert_eq!(
            items.iter().map(|item| item.class).collect::<Vec<_>>(),
            vec![0x0082, 0x0080, 0x0080, 0x0080, 0x0080]
        );
        // 提示占满整行宽度，选项一行一个。
        assert_eq!(items[0].rect, [7, 7, width - 14, 18]);
        assert_eq!(items[1].rect, [10, 31, width - 17, 12]);
        assert_eq!(items[2].rect, [10, 43, width - 17, 12]);

        // 标题、提示、每个选项都要原样落进模板。
        for text in ["选择处理方式", "已存在同名文件，怎么处理？", "覆盖", "跳过"]
        {
            assert!(contains_text(&bytes, text), "{text} 没进模板");
        }
    }

    #[test]
    fn choice_template_keeps_every_item_inside_the_frame() {
        for rows in 1..=6usize {
            let options = vec!["选项".to_string(); rows];

            let bytes = win::choice_template("标题", "提示", &options);
            let width = read_u16(&bytes, 14) as i16;
            let height = read_u16(&bytes, 16) as i16;
            for item in template_items(&bytes) {
                let [x, y, cx, cy] = item.rect;
                assert!(x >= 0 && y >= 0 && cx > 0 && cy > 0, "{rows} 行时矩形非法");
                assert!(x + cx <= width, "{rows} 行时控件越过右边框");
                assert!(y + cy <= height, "{rows} 行时控件越过下边框");
            }
        }
    }

    #[test]
    fn aligned_u32_keeps_the_template_bytes_in_order() {
        let bytes = win::choice_template("标题", "提示", &labels(&["a"]));
        let flat: Vec<u8> = win::aligned_u32(&bytes)
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        assert_eq!(&flat[..bytes.len()], &bytes[..]);
        assert!(flat.len() - bytes.len() < 4, "最多补到 4 的边界");
    }

    fn labels(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    /// 按 `DLGTEMPLATE` 的字段顺序走一遍模板：对不上就说明字段长度拼错了，
    /// 系统那边只会得到一个弹不出来的窗口。
    fn template_items(bytes: &[u8]) -> Vec<Item> {
        let count = read_u16(bytes, 8) as usize;
        let mut at = skip_text(bytes, 22); // 头 18 字节 + 菜单 2 + 类 2，随后是标题
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            // 子控件模板必须 4 字节对齐，所以标题之后、每个控件之后都可能带填充字节。
            at = at.next_multiple_of(4);
            let mut rect = [0i16; 4];
            for (slot, offset) in rect.iter_mut().zip([8, 10, 12, 14]) {
                *slot = read_u16(bytes, at + offset) as i16;
            }
            items.push(Item {
                class: read_u16(bytes, at + 20),
                id: read_u16(bytes, at + 16),
                rect,
            });
            at = skip_text(bytes, at + 22) + 2; // 标题之后是「创建数据长度」
        }
        items
    }

    fn read_u16(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn skip_text(bytes: &[u8], mut at: usize) -> usize {
        while read_u16(bytes, at) != 0 {
            at += 2;
        }
        at + 2
    }

    fn contains_text(bytes: &[u8], text: &str) -> bool {
        let needle: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        bytes.windows(needle.len()).any(|window| window == needle)
    }
}
