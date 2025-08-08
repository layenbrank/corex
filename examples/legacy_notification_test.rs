use fluxor::LegacyNotificationHelper;

fn main() {
    println!("🚀 传统桌面应用通知测试");
    println!("================================");

    // 检查通知系统状态
    println!("📊 通知系统状态:");
    print!("{}", LegacyNotificationHelper::check_notification_system());

    println!("\n🔔 尝试显示Toast通知...");
    match LegacyNotificationHelper::show_desktop_notification(
        "Fluxor 桌面通知",
        "这是一个传统桌面应用的通知测试",
    ) {
        Ok(_) => {
            println!("✅ Toast通知发送成功!");
            println!("请检查屏幕右下角的通知区域");
        }
        Err(e) => {
            println!("❌ Toast通知失败: {:?}", e);
            println!("🔄 尝试使用备用方案...");

            // 使用备用的消息框方案
            if let Err(e2) = LegacyNotificationHelper::show_balloon_tip(
                "Fluxor 通知",
                "Toast通知失败，使用消息框显示",
            ) {
                println!("❌ 备用方案也失败了: {:?}", e2);
            } else {
                println!("✅ 使用消息框显示成功!");
            }
        }
    }

    println!("\n💡 如果看不到通知，请尝试:");
    println!("   1. 打开 Windows 设置 > 系统 > 通知和操作");
    println!("   2. 确保 '获取来自应用和其他发送者的通知' 已启用");
    println!("   3. 检查 '专注助手' 设置");
    println!("   4. 以管理员身份运行此程序");
    println!("   5. 重启 Windows 通知服务");

    println!("\n⏰ 等待5秒，观察通知...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    println!("🏁 测试完成");
}
