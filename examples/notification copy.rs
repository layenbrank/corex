use anyhow::{Context, Result as AnyhowResult};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use windows::{core::*, Data::Xml::Dom::*, Win32::UI::Shell::*, UI::Notifications::*};

/// Windows Toast 通知工具
///
/// 支持从XML文件或模板创建通知
#[derive(Debug, Clone)]
pub struct Notification {
    app_id: String,
    title: Option<String>,
    content: Option<String>,
    xml_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub app_id: String,
    pub icon_path: Option<String>,
    pub sound_enabled: bool,
    pub duration: ToastDuration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToastDuration {
    Short,
    Long,
}

impl NotificationConfig {
    /// 从JSON文件加载配置
    pub fn from_file<P: AsRef<Path>>(config_path: P) -> AnyhowResult<Self> {
        let config_content = fs::read_to_string(config_path.as_ref())
            .with_context(|| format!("无法读取配置文件: {}", config_path.as_ref().display()))?;

        let config: serde_json::Value =
            serde_json::from_str(&config_content).with_context(|| "配置文件JSON格式错误")?;

        let notification_config = config
            .get("notification")
            .ok_or_else(|| anyhow::anyhow!("配置文件中未找到 'notification' 节点"))?;

        let duration_str = notification_config
            .get("duration")
            .and_then(|v| v.as_str())
            .unwrap_or("short");

        let duration = match duration_str {
            "long" => ToastDuration::Long,
            _ => ToastDuration::Short,
        };

        Ok(Self {
            app_id: notification_config
                .get("app_id")
                .and_then(|v| v.as_str())
                .unwrap_or("com.fluxor.corex")
                .to_string(),
            icon_path: notification_config
                .get("icon_path")
                .and_then(|v| v.as_str())
                .map(String::from),
            sound_enabled: notification_config
                .get("sound_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            duration,
        })
    }

    /// 保存配置到JSON文件
    pub fn save_to_file<P: AsRef<Path>>(&self, config_path: P) -> AnyhowResult<()> {
        let duration_str = match self.duration {
            ToastDuration::Short => "short",
            ToastDuration::Long => "long",
        };

        let config = serde_json::json!({
            "notification": {
                "app_id": self.app_id,
                "sound_enabled": self.sound_enabled,
                "duration": duration_str,
                "icon_path": self.icon_path
            }
        });

        let config_content =
            serde_json::to_string_pretty(&config).with_context(|| "序列化配置失败")?;

        fs::write(config_path.as_ref(), config_content)
            .with_context(|| format!("写入配置文件失败: {}", config_path.as_ref().display()))?;

        Ok(())
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            app_id: "com.fluxor.corex".to_string(),
            icon_path: None,
            sound_enabled: true,
            duration: ToastDuration::Short,
        }
    }
}

impl Notification {
    /// 创建新的通知实例
    pub fn new() -> Self {
        Self {
            app_id: "com.fluxor.corex".to_string(),
            title: None,
            content: None,
            xml_template: None,
        }
    }

    /// 使用配置创建通知实例
    pub fn with_config(config: NotificationConfig) -> Self {
        Self {
            app_id: config.app_id,
            title: None,
            content: None,
            xml_template: None,
        }
    }

    /// 设置标题
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 设置内容
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// 从XML文件加载模板
    pub fn from_xml_file<P: AsRef<Path>>(mut self, xml_path: P) -> AnyhowResult<Self> {
        let xml_content = fs::read_to_string(xml_path.as_ref())
            .with_context(|| format!("无法读取XML文件: {}", xml_path.as_ref().display()))?;

        self.xml_template = Some(xml_content);
        Ok(self)
    }

    /// 从XML字符串加载模板
    pub fn from_xml_string(mut self, xml_content: impl Into<String>) -> Self {
        self.xml_template = Some(xml_content.into());
        self
    }

    /// 生成基础Toast XML模板
    fn generate_basic_xml(&self) -> String {
        let title = self.title.as_deref().unwrap_or("Fluxor 通知");
        let content = self.content.as_deref().unwrap_or("来自 Fluxor 的消息");

        format!(
            r#"<toast>
    <visual>
        <binding template="ToastGeneric">
            <text id="1">{}</text>
            <text id="2">{}</text>
        </binding>
    </visual>
    <audio silent="false"/>
</toast>"#,
            Self::escape_xml(title),
            Self::escape_xml(content)
        )
    }

    /// 生成高级Toast XML模板
    fn generate_advanced_xml(&self, config: &NotificationConfig) -> String {
        let title = self.title.as_deref().unwrap_or("Fluxor 通知");
        let content = self.content.as_deref().unwrap_or("来自 Fluxor 的消息");

        let duration = match config.duration {
            ToastDuration::Short => "short",
            ToastDuration::Long => "long",
        };

        let audio_section = if config.sound_enabled {
            r#"<audio src="ms-winsoundevent:Notification.Default"/>"#
        } else {
            r#"<audio silent="true"/>"#
        };

        let image_section = if let Some(ref icon_path) = config.icon_path {
            format!(
                r#"<image placement="appLogoOverride" src="{}"/>"#,
                icon_path
            )
        } else {
            String::new()
        };

        format!(
            r#"<toast duration="{}">
    <visual>
        <binding template="ToastGeneric">
            {}
            <text id="1">{}</text>
            <text id="2">{}</text>
        </binding>
    </visual>
    {}
</toast>"#,
            duration,
            image_section,
            Self::escape_xml(title),
            Self::escape_xml(content),
            audio_section
        )
    }

    /// XML字符转义
    fn escape_xml(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    /// 显示通知（使用默认配置）
    pub fn show(&self) -> Result<()> {
        self.show_with_config(&NotificationConfig::default())
    }

    /// 使用指定配置显示通知
    pub fn show_with_config(&self, config: &NotificationConfig) -> Result<()> {
        // 设置应用ID
        let app_id = HSTRING::from(&config.app_id);

        unsafe {
            if let Err(e) = SetCurrentProcessExplicitAppUserModelID(&app_id) {
                eprintln!("⚠️  AppUserModelID 设置失败: {:?}", e);
            }
        }

        // 获取XML内容
        let xml_content = if let Some(ref template) = self.xml_template {
            template.clone()
        } else {
            self.generate_advanced_xml(config)
        };

        // 创建XML文档
        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(&xml_content))?;

        // 创建Toast通知
        let toast = ToastNotification::CreateToastNotification(&xml_doc)?;

        // 显示通知
        match ToastNotificationManager::CreateToastNotifierWithId(&app_id) {
            Ok(notifier) => {
                notifier.Show(&toast)?;
                println!("🎉 通知显示成功!");
                Ok(())
            }
            Err(e) => {
                eprintln!("❌ 通知显示失败: {:?}", e);
                Err(e)
            }
        }
    }

    /// 快速显示简单通知的便捷方法
    pub fn quick_show(title: &str, content: &str) -> Result<()> {
        Self::new().title(title).content(content).show()
    }

    /// 从XML文件快速显示通知
    pub fn show_from_xml_file<P: AsRef<Path>>(xml_path: P) -> AnyhowResult<()> {
        let notification = Self::new().from_xml_file(xml_path)?;

        notification
            .show()
            .map_err(|e| anyhow::anyhow!("显示通知失败: {:?}", e))
    }

    /// 从配置文件加载并显示通知
    pub fn show_with_config_file<P: AsRef<Path>>(
        config_path: P,
        title: &str,
        content: &str,
    ) -> AnyhowResult<()> {
        let config = NotificationConfig::from_file(config_path)?;

        Self::new()
            .title(title)
            .content(content)
            .show_with_config(&config)
            .map_err(|e| anyhow::anyhow!("显示通知失败: {:?}", e))
    }

    /// 替换XML模板中的占位符
    pub fn replace_placeholders(
        mut self,
        placeholders: std::collections::HashMap<String, String>,
    ) -> Self {
        if let Some(ref mut template) = self.xml_template {
            for (key, value) in placeholders {
                *template = template.replace(&format!("{{{}}}", key), &Self::escape_xml(&value));
            }
        }
        self
    }
}
