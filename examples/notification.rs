use windows::{
    core::*,
    Data::Xml::Dom::*,
    Win32::{
        Foundation::*,
        System::Services::*,
        UI::{Shell::Shell, WindowsAndMessaging::*, *},
    },
    UI::Notifications::*,
};

pub struct Notification;

impl Notification {
    pub fn new(title: &str, content: &str) -> Result<()> {
        let app_id = HSTRING::from("com.fluxor.corex");

        unsafe {
            match Shell::SetCurrentProcessExplicitAppUserModelID(&app_id) {
                Ok(_) => println!("✅ AppUserModelID 设置成功: {}", app_id),
                Err(e) => println!("❌ AppUserModelID 设置失败: {:?}", e),
            }
        }

        // 创建包含标题和内容的Toast XML模板
        let toast_xml = format!(
            r#"<toast>
    <visual>
        <binding template="ToastText02">
            <text id="1">{}</text>
            <text id="2">{}</text>
        </binding>
    </visual>
    <audio silent="false"/>
</toast>"#,
            Self::escape_xml(title),
            Self::escape_xml(content)
        );

        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(toast_xml))?;
        println!("✅ XML文档创建成功");

        let toast = ToastNotification::CreateToastNotification(&xml_doc)?;
        println!("✅ Toast通知对象创建成功");

        println!("📄 标题: {}", title);
        println!("📄 内容: {}", content);

        // 使用指定的AppId创建通知器
        match ToastNotificationManager::CreateToastNotifierWithId(&app_id) {
            Ok(notifier) => {
                println!("✅ 通知器创建成功");
                match notifier.Show(&toast) {
                    Ok(_) => println!("🎉 通知成功!"),
                    Err(e) => println!("❌ 通知失败: {:?}", e),
                }
            }
            Err(e) => println!("❌ 通知器创建失败: {:?}", e),
        }

        Ok(())
    }

    /// 转义XML特殊字符以防止XML解析错误
    fn escape_xml(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    /// 创建只有内容没有标题的简单通知
    pub fn simple(content: &str) -> Result<()> {
        Self::new("通知", content)
    }

    /// 创建成功通知
    pub fn success(title: &str, content: &str) -> Result<()> {
        let success_xml = format!(
            r#"<toast>
    <visual>
        <binding template="ToastText02">
            <text id="1">✅ {}</text>
            <text id="2">{}</text>
        </binding>
    </visual>
    <audio src="ms-winsoundevent:Notification.Default"/>
</toast>"#,
            Self::escape_xml(title),
            Self::escape_xml(content)
        );

        Self::show_toast(&success_xml)
    }

    /// 创建错误通知
    pub fn error(title: &str, content: &str) -> Result<()> {
        let error_xml = format!(
            r#"<toast>
    <visual>
        <binding template="ToastText02">
            <text id="1">❌ {}</text>
            <text id="2">{}</text>
        </binding>
    </visual>
    <audio src="ms-winsoundevent:Notification.Looping.Alarm"/>
</toast>"#,
            Self::escape_xml(title),
            Self::escape_xml(content)
        );

        Self::show_toast(&error_xml)
    }

    /// 内部方法：显示Toast通知
    fn show_toast(xml_content: &str) -> Result<()> {
        let app_id = HSTRING::from("com.fluxor.corex");

        unsafe {
            Shell::SetCurrentProcessExplicitAppUserModelID(&app_id).ok();
        }

        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(xml_content))?;

        let toast = ToastNotification::CreateToastNotification(&xml_doc)?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&app_id)?;

        notifier.Show(&toast)?;
        Ok(())
    }

    pub fn show(&self) {
        // 保留这个方法以保持接口兼容性
        println!("显示通知");
    }
}
