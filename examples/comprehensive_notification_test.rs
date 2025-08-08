use fluxor::LegacyNotificationHelper;
use std::thread;
use std::time::Duration;

fn wait_with_countdown(seconds: u64, message: &str) {
    for i in (1..=seconds).rev() {
        print!("\r{} ({}秒)", message, i);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        thread::sleep(Duration::from_secs(1));
    }
    println!("\r{} 完成!", message);
}

fn main() {
    println!("🎯 强化通知测试程序");
    println!("====================");

    println!("🔧 正在配置通知系统...");

    // 测试多种通知方式
    println!("\n📢 测试1: 基础桌面通知");
    match LegacyNotificationHelper::show_desktop_notification(
        "Fluxor 通知测试",
        "如果你看到这个通知，说明系统配置正确！",
    ) {
        Ok(_) => println!("✅ 发送成功"),
        Err(e) => println!("❌ 发送失败: {:?}", e),
    }

    wait_with_countdown(3, "⏰ 等待通知显示");

    println!("\n📢 测试2: 连续通知");
    for i in 1..=3 {
        match LegacyNotificationHelper::show_desktop_notification(
            &format!("通知 #{}", i),
            &format!("这是第{}个测试通知", i),
        ) {
            Ok(_) => println!("✅ 通知{}发送成功", i),
            Err(e) => println!("❌ 通知{}发送失败: {:?}", i, e),
        }
        thread::sleep(Duration::from_millis(500));
    }

    wait_with_countdown(5, "⏰ 等待查看连续通知");

    println!("\n📢 测试3: 备用通知方案（消息框）");
    println!("如果Toast通知不工作，这个一定会显示：");

    if let Err(e) = LegacyNotificationHelper::show_balloon_tip(
        "Fluxor 备用通知",
        "这是使用Win32 MessageBox的备用通知方案",
    ) {
        println!("❌ 连备用方案都失败了: {:?}", e);
    }

    println!("\n🎯 测试结果分析:");
    println!("================================");

    print!("{}", LegacyNotificationHelper::check_notification_system());

    println!("\n🔍 故障排除步骤:");
    println!("1. ✅ 检查通知是否在操作中心/通知面板中");
    println!("2. 🔧 Windows设置 > 系统 > 通知 > 确保通知已启用");
    println!("3. 🚫 检查是否开启了专注助手/勿扰模式");
    println!("4. 📅 检查通知历史记录 (Win+A 打开操作中心)");
    println!("5. 🔄 重启 Windows 通知服务:");
    println!("   - 按 Win+R，输入 services.msc");
    println!("   - 找到 'Windows Push Notifications User Service'");
    println!("   - 右键重启服务");

    println!("\n🎉 测试完成！");
}
