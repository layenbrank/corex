use windows::{core::*, Data::Xml::Dom::*, Foundation::*, UI::Notifications::*};

pub struct NotificationHelper;

impl NotificationHelper {
    /// 创建一个简单的Toast通知
    pub fn create_toast_notification(title: &str, message: &str) -> Result<ToastNotification> {
        // 创建Toast通知的XML内容
        let toast_xml = format!(
            r#"<toast>
                <visual>
                    <binding template="ToastGeneric">
                        <text>{}</text>
                        <text>{}</text>
                    </binding>
                </visual>
            </toast>"#,
            title, message
        );

        // 创建XML文档
        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(toast_xml))?;

        // 创建Toast通知
        let toast_notification = ToastNotification::CreateToastNotification(&xml_doc)?;

        Ok(toast_notification)
    }

    /// 显示Toast通知
    pub fn show_toast_notification(title: &str, message: &str) -> Result<()> {
        let toast = Self::create_toast_notification(title, message)?;

        // 获取Toast通知管理器
        let toast_notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("fluxor"))?;

        // 显示通知
        toast_notifier.Show(&toast)?;

        Ok(())
    }

    /// 创建磁贴通知
    pub fn create_tile_notification(content: &str) -> Result<TileNotification> {
        let tile_xml = format!(
            r#"<tile>
                <visual>
                    <binding template="TileMedium">
                        <text hint-wrap="true">{}</text>
                    </binding>
                </visual>
            </tile>"#,
            content
        );

        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(tile_xml))?;

        let tile_notification = TileNotification::CreateTileNotification(&xml_doc)?;

        Ok(tile_notification)
    }

    /// 更新磁贴
    pub fn update_tile(content: &str) -> Result<()> {
        let tile = Self::create_tile_notification(content)?;

        // 获取磁贴更新器
        let tile_updater = TileUpdateManager::CreateTileUpdaterForApplication()?;

        // 更新磁贴
        tile_updater.Update(&tile)?;

        Ok(())
    }

    /// 创建定时Toast通知
    pub fn schedule_toast_notification(
        title: &str,
        message: &str,
        delay_seconds: i64,
    ) -> Result<()> {
        let toast_xml = format!(
            r#"<toast>
                <visual>
                    <binding template="ToastGeneric">
                        <text>{}</text>
                        <text>{}</text>
                    </binding>
                </visual>
            </toast>"#,
            title, message
        );

        let xml_doc = XmlDocument::new()?;
        xml_doc.LoadXml(&HSTRING::from(toast_xml))?;

        // 使用系统时间创建DateTime
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let future_time = now + delay_seconds;

        // Windows FILETIME 格式: 从1601年1月1日开始的100纳秒间隔数
        // Unix时间戳从1970年1月1日开始，需要转换
        let windows_epoch_diff = 11644473600i64; // 1601到1970的秒数
        let filetime = (future_time + windows_epoch_diff) * 10_000_000i64; // 转换为100纳秒

        let delivery_time = DateTime {
            UniversalTime: filetime,
        };

        let scheduled_toast =
            ScheduledToastNotification::CreateScheduledToastNotification(&xml_doc, delivery_time)?;

        let toast_notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("fluxor"))?;
        toast_notifier.AddToSchedule(&scheduled_toast)?;

        Ok(())
    }

    /// 清除所有通知
    pub fn clear_all_notifications() -> Result<()> {
        // 获取所有已显示的通知
        let history = ToastNotificationManager::History()?;
        history.Clear()?;

        Ok(())
    }

    /// 获取通知权限状态
    pub fn get_notification_setting() -> Result<NotificationSetting> {
        let toast_notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("fluxor"))?;
        let setting = toast_notifier.Setting()?;
        Ok(setting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_toast_notification() {
        let result = NotificationHelper::create_toast_notification("测试标题", "测试消息");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_tile_notification() {
        let result = NotificationHelper::create_tile_notification("磁贴内容");
        assert!(result.is_ok());
    }
}
