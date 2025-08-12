use crate::utils::notification::{Notification, NotificationConfig, ToastDuration};

/// 演示新通知系统的各种用法
pub fn demo_notifications() -> anyhow::Result<()> {
    println!("🚀 开始演示 Fluxor 通知系统...\n");

    // 1. 简单快速通知
    println!("1️⃣ 显示简单快速通知");
    Notification::quick_show("测试标题", "这是一条测试消息")?;
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 2. 链式构建通知
    println!("2️⃣ 使用链式构建显示通知");
    Notification::new()
        .title("链式构建")
        .content("这是通过链式方法构建的通知")
        .show()?;
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 3. 使用自定义配置
    println!("3️⃣ 使用自定义配置显示通知");
    let config = NotificationConfig {
        app_id: "com.fluxor.custom".to_string(),
        icon_path: Some("file:///C:/Windows/System32/SecurityAndMaintenance.png".to_string()),
        sound_enabled: true,
        duration: ToastDuration::Long,
    };

    Notification::with_config(config)
        .title("自定义配置")
        .content("这是使用自定义配置的通知")
        .show()?;
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 4. 从XML文件加载
    println!("4️⃣ 从XML文件加载通知模板");
    if let Err(e) = Notification::show_from_xml_file("templates/notification_basic.xml") {
        println!("❌ 从XML文件加载失败: {}", e);
        println!("💡 提示: 请确保 templates/notification_basic.xml 文件存在");
    }
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 5. 从XML字符串创建
    println!("5️⃣ 从XML字符串创建通知");
    let custom_xml = r#"<toast>
        <visual>
            <binding template="ToastGeneric">
                <text id="1">自定义XML</text>
                <text id="2">这是直接从XML字符串创建的通知</text>
            </binding>
        </visual>
        <audio silent="false"/>
    </toast>"#;

    Notification::new().from_xml_string(custom_xml).show()?;

    println!("\n✅ 通知系统演示完成!");
    Ok(())
}

/// 任务完成通知
pub fn notify_task_complete(
    task_name: &str,
    file_count: usize,
    success: bool,
) -> anyhow::Result<()> {
    let (title, content, icon) = if success {
        (
            format!("✅ {} 完成", task_name),
            format!("成功处理了 {} 个文件", file_count),
            "ms-appx:///Assets/success.png",
        )
    } else {
        (
            format!("❌ {} 失败", task_name),
            "处理过程中出现错误".to_string(),
            "ms-appx:///Assets/error.png",
        )
    };

    let config = NotificationConfig {
        app_id: "com.fluxor.tasks".to_string(),
        icon_path: Some(icon.to_string()),
        sound_enabled: true,
        duration: ToastDuration::Long,
    };

    Notification::with_config(config)
        .title(&title)
        .content(&content)
        .show()?;

    Ok(())
}

/// 文件操作进度通知
pub fn notify_file_progress(operation: &str, current: usize, total: usize) -> anyhow::Result<()> {
    let progress = (current as f64 / total as f64 * 100.0) as u8;

    Notification::new()
        .title(format!("📁 {}", operation))
        .content(format!("进度: {}/{} ({}%)", current, total, progress))
        .show()?;

    Ok(())
}
