use fluxor::NotificationHelper;
use windows::{core::*, UI::Notifications::*};

fn check_app_registration() {
    println!("=== 检查应用注册状态 ===");

    // 尝试获取当前进程信息
    println!(
        "当前进程: {}",
        std::env::current_exe().unwrap_or_default().display()
    );
    println!("命令行参数: {:?}", std::env::args().collect::<Vec<_>>());

    // 检查是否从打包应用运行
    if std::env::var("APPDATA").is_ok() {
        println!("运行环境: 传统桌面应用");
    }
}

fn test_basic_notification() {
    println!("\n=== 测试基础通知 ===");

    // 尝试使用默认的通知器（不指定应用ID）
    match ToastNotificationManager::CreateToastNotifier() {
        Ok(notifier) => {
            println!("默认通知器创建成功");

            // 创建简单的Toast通知
            match NotificationHelper::create_toast_notification("简单测试", "这是一个基础测试通知")
            {
                Ok(toast) => match notifier.Show(&toast) {
                    Ok(_) => println!("通知显示成功!"),
                    Err(e) => println!("通知显示失败: {:?}", e),
                },
                Err(e) => println!("创建通知失败: {:?}", e),
            }
        }
        Err(e) => println!("创建默认通知器失败: {:?}", e),
    }
}

fn test_notification_permissions() {
    println!("\n=== 检查通知权限 ===");

    // 检查系统通知设置
    match ToastNotificationManager::CreateToastNotifier() {
        Ok(notifier) => {
            let setting = notifier.Setting().unwrap_or(NotificationSetting::Enabled);
            match setting {
                NotificationSetting::Enabled => println!("✅ 通知已启用"),
                NotificationSetting::DisabledForApplication => println!("❌ 应用通知被禁用"),
                NotificationSetting::DisabledForUser => println!("❌ 用户通知被禁用"),
                NotificationSetting::DisabledByGroupPolicy => println!("❌ 组策略禁用通知"),
                NotificationSetting::DisabledByManifest => println!("❌ 清单文件禁用通知"),
                _ => println!("⚠️ 未知的通知设置状态: {:?}", setting),
            }
        }
        Err(e) => println!("获取通知设置失败: {:?}", e),
    }
}

fn test_enhanced_toast() {
    println!("\n=== 测试增强Toast通知 ===");

    // 创建包含更多元素的Toast通知
    let enhanced_xml = r#"<toast launch="app-defined-string">
        <visual>
            <binding template="ToastGeneric">
                <image placement="appLogoOverride" hint-crop="circle" src="ms-appx:///Assets/andrew.jpg"/>
                <text>Fluxor 通知测试</text>
                <text>这是一个增强的通知，包含更多功能</text>
                <text placement="attribution">来自 Fluxor</text>
            </binding>
        </visual>
        <actions>
            <action content="确定" arguments="ok"/>
            <action content="取消" arguments="cancel"/>
        </actions>
        <audio src="ms-winsoundevent:Notification.Default"/>
    </toast>"#;

    match windows::Data::Xml::Dom::XmlDocument::new() {
        Ok(xml_doc) => match xml_doc.LoadXml(&HSTRING::from(enhanced_xml)) {
            Ok(_) => match ToastNotification::CreateToastNotification(&xml_doc) {
                Ok(toast) => match ToastNotificationManager::CreateToastNotifier() {
                    Ok(notifier) => match notifier.Show(&toast) {
                        Ok(_) => println!("增强通知显示成功!"),
                        Err(e) => println!("增强通知显示失败: {:?}", e),
                    },
                    Err(e) => println!("创建通知器失败: {:?}", e),
                },
                Err(e) => println!("创建增强通知失败: {:?}", e),
            },
            Err(e) => println!("加载XML失败: {:?}", e),
        },
        Err(e) => println!("创建XML文档失败: {:?}", e),
    }
}

fn main() {
    println!("🔍 Windows 通知系统诊断工具");
    println!("==========================================");

    // 检查应用注册
    check_app_registration();

    // 检查通知权限
    test_notification_permissions();

    // 测试基础通知
    test_basic_notification();

    // 等待一下
    println!("\n⏱️ 等待3秒...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    // 测试增强通知
    test_enhanced_toast();

    println!("\n📋 故障排除建议:");
    println!("1. 检查 Windows 设置 > 系统 > 通知和操作");
    println!("2. 确保允许应用发送通知");
    println!("3. 检查专注助手(勿扰模式)设置");
    println!("4. 尝试以管理员身份运行");
    println!("5. 重启 Windows 通知服务: services.msc -> Windows Push Notifications User Service");

    println!("\n⏰ 等待10秒观察通知...");
    std::thread::sleep(std::time::Duration::from_secs(10));
}
