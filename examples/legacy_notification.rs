use windows::{core::*, Data::Xml::Dom::*, Win32::UI::Shell::*, UI::Notifications::*};

pub struct LegacyNotificationHelper;

impl LegacyNotificationHelper {
    /// 为传统桌面应用创建通知
    pub fn show_desktop_notification(title: &str, message: &str) -> Result<()> {
        // 方法1: 尝试使用应用程序用户模型ID
        if let Ok(()) = Self::try_with_app_id(title, message) {
            return Ok(());
        }

        // 方法2: 尝试使用系统默认方式
        Self::try_system_default(title, message)
    }

    fn try_with_app_id(title: &str, message: &str) -> Result<()> {
        // 设置应用程序用户模型ID (这对传统桌面应用很重要)
        let app_id = HSTRING::from("Fluxor.DesktopApp.1");

        unsafe {
            // 为当前进程设置AppUserModelId
            SetCurrentProcessExplicitAppUserModelID(&app_id)?;
        }

        // 创建Toast通知
        let toast_xml = format!(
            r#"<toast>
                <visual>
                    <binding template="ToastGeneric">
                        <text>{}</text>
                        <text>{}</text>
                    </binding>
                </visual>
                <audio silent="false" />
            </toast>"#,
            title, message
        );

        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(toast_xml))?;

        let toast = ToastNotification::CreateToastNotification(&xml_doc)?;

        // 使用指定的AppId创建通知器
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&app_id)?;
        notifier.Show(&toast)?;

        Ok(())
    }

    fn try_system_default(title: &str, message: &str) -> Result<()> {
        // 创建更兼容的Toast通知XML
        let toast_xml = format!(
            r#"<toast>
                <visual>
                    <binding template="ToastText02">
                        <text id="1">{}</text>
                        <text id="2">{}</text>
                    </binding>
                </visual>
            </toast>"#,
            title, message
        );

        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(toast_xml))?;

        let toast = ToastNotification::CreateToastNotification(&xml_doc)?;

        // 尝试使用默认通知器
        let notifier = ToastNotificationManager::CreateToastNotifier()?;
        notifier.Show(&toast)?;

        Ok(())
    }

    /// 使用Win32 API显示气球提示
    pub fn show_balloon_tip(title: &str, message: &str) -> Result<()> {
        use windows::Win32::Foundation::*;
        use windows::Win32::UI::WindowsAndMessaging::*;

        // 这是一个备用方案，使用Windows的MessageBox
        let title_wide = HSTRING::from(title);
        let message_wide = HSTRING::from(format!("{}\n\n(这是一个测试通知)", message));

        unsafe {
            MessageBoxW(
                None,
                &message_wide,
                &title_wide,
                MB_ICONINFORMATION | MB_OK | MB_TOPMOST,
            );
        }

        Ok(())
    }

    /// 检查通知系统状态
    pub fn check_notification_system() -> String {
        let mut status = String::new();

        // 检查是否可以创建通知管理器
        match ToastNotificationManager::CreateToastNotifier() {
            Ok(notifier) => {
                status.push_str("✅ 通知管理器创建成功\n");

                match notifier.Setting() {
                    Ok(setting) => match setting {
                        NotificationSetting::Enabled => status.push_str("✅ 通知已启用\n"),
                        NotificationSetting::DisabledForApplication => {
                            status.push_str("❌ 应用通知被禁用\n")
                        }
                        NotificationSetting::DisabledForUser => {
                            status.push_str("❌ 用户通知被禁用\n")
                        }
                        NotificationSetting::DisabledByGroupPolicy => {
                            status.push_str("❌ 组策略禁用通知\n")
                        }
                        NotificationSetting::DisabledByManifest => {
                            status.push_str("❌ 清单文件禁用通知\n")
                        }
                        _ => status.push_str(&format!("⚠️ 未知通知状态: {:?}\n", setting)),
                    },
                    Err(e) => status.push_str(&format!("❌ 获取通知设置失败: {:?}\n", e)),
                }
            }
            Err(e) => status.push_str(&format!("❌ 通知管理器创建失败: {:?}\n", e)),
        }

        status
    }
}
