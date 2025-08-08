use fluxor::LegacyNotificationHelper;
use std::thread;
use std::time::Duration;

fn main() {
    println!("🎯 最终通知验证测试");
    println!("===================");

    println!("准备发送一系列明显的通知...");
    println!("请注意屏幕右下角！");

    for i in 5..=1 {
        println!("{}秒后发送通知...", i);
        thread::sleep(Duration::from_secs(1));
    }

    // 发送一系列通知，每个都有不同的内容
    let notifications = vec![
        ("🔥 重要通知", "这是第1个测试通知 - 请查看右下角！"),
        ("⚡ 紧急消息", "这是第2个测试通知 - 应该在屏幕角落显示"),
        ("🎉 成功！", "这是第3个测试通知 - 如果看到了说明成功了！"),
        ("📢 最后一个", "这是最后一个测试通知 - 检查操作中心 (Win+A)"),
    ];

    for (i, (title, message)) in notifications.iter().enumerate() {
        println!("\n发送通知 {} / {}:", i + 1, notifications.len());
        println!("标题: {}", title);
        println!("内容: {}", message);

        match LegacyNotificationHelper::show_desktop_notification(title, message) {
            Ok(_) => {
                println!("✅ 发送成功！");
                println!("👀 请立即查看屏幕右下角！");
            }
            Err(e) => println!("❌ 发送失败: {:?}", e),
        }

        // 等待足够长的时间让通知显示
        for j in 3..=1 {
            print!("\r⏰ 等待 {} 秒查看通知...", j);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            thread::sleep(Duration::from_secs(1));
        }
        println!("\r                                    "); // 清除行
    }

    println!("\n🔍 如果你没有看到任何通知，请：");
    println!("1. 按 Win + A 打开操作中心/通知面板");
    println!("2. 查看是否有来自 'Fluxor.NotificationTest.1' 的通知");
    println!("3. 检查通知设置 (设置 > 系统 > 通知)");
    println!("4. 确保没有开启专注助手/勿扰模式");

    println!("\n✨ 测试完成！如果通知正常工作，你应该看到了4个不同的通知。");
}
