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

        // 创建最简单的Toast XML
        let simple_toast_xml = r#"<toast>
        <visual>
            <binding template="ToastText01">
                <text id="1">Hello from Fluxor!</text>
            </binding>
        </visual>
        <audio silent="false"/>
    </toast>"#;

        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(simple_toast_xml))?;
        println!("✅ XML文档创建成功");

        let toast = ToastNotification::CreateToastNotification(&xml_doc)?;
        println!("✅ Toast通知对象创建成功");

        // 5. 尝试不同的通知器创建方法
        println!("\n📢 尝试显示通知...");

        // 方法1: 使用指定的AppId
        match ToastNotificationManager::CreateToastNotifierWithId(&app_id) {
            Ok(notifier) => {
                println!("✅ 使用AppId的通知器创建成功");
                match notifier.Show(&toast) {
                    Ok(_) => println!("🎉 通知显示成功! (使用AppId)"),
                    Err(e) => println!("❌ 通知显示失败: {:?}", e),
                }
            }
            Err(e) => println!("❌ 使用AppId的通知器创建失败: {:?}", e),
        }

        Ok(())
    }

    pub fn show(&self) {
        // Show the notification
    }
}
