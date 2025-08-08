use fluxor::NotificationHelper;

fn main() {
    println!("测试Windows通知功能...");

    // 测试Toast通知
    match NotificationHelper::show_toast_notification("Fluxor测试", "这是一个测试通知") {
        Ok(_) => println!("Toast通知发送成功!"),
        Err(e) => eprintln!("Toast通知发送失败: {:?}", e),
    }

    // 测试磁贴更新
    match NotificationHelper::update_tile("Fluxor正在运行") {
        Ok(_) => println!("磁贴更新成功!"),
        Err(e) => eprintln!("磁贴更新失败: {:?}", e),
    }

    // 检查通知权限
    match NotificationHelper::get_notification_setting() {
        Ok(setting) => println!("通知设置状态: {:?}", setting),
        Err(e) => eprintln!("获取通知设置失败: {:?}", e),
    }

    // 测试定时通知（5秒后）
    match NotificationHelper::schedule_toast_notification("定时通知", "这是一个5秒延迟的通知", 5)
    {
        Ok(_) => println!("定时通知已设置!"),
        Err(e) => eprintln!("定时通知设置失败: {:?}", e),
    }

    println!("通知测试完成，等待5秒查看定时通知...");
    std::thread::sleep(std::time::Duration::from_secs(6));
}
