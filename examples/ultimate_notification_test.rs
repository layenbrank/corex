use windows::{
    core::*, Data::Xml::Dom::*, Win32::Foundation::*, Win32::System::Services::*,
    Win32::UI::Shell::*, Win32::UI::WindowsAndMessaging::*, UI::Notifications::*,
};

fn main() -> Result<()> {
    println!("🛠️  Windows 通知系统完整诊断");
    println!("=====================================");

    // 1. 检查操作系统版本
    println!("📋 系统信息:");
    println!("   OS: {}", std::env::consts::OS);
    println!("   Arch: {}", std::env::consts::ARCH);

    // 2. 尝试设置AppUserModelId
    println!("\n🏷️  设置应用程序用户模型ID...");
    let app_id = HSTRING::from("Fluxor.NotificationTest.1");
    unsafe {
        match SetCurrentProcessExplicitAppUserModelID(&app_id) {
            Ok(_) => println!("✅ AppUserModelID 设置成功: {}", app_id),
            Err(e) => println!("❌ AppUserModelID 设置失败: {:?}", e),
        }
    }

    // 3. 显示一个简单的MessageBox确认GUI工作
    println!("\n📦 测试基础GUI功能...");
    unsafe {
        let result = MessageBoxW(
            None,
            &HSTRING::from("这是一个测试消息框。\n如果你看到这个，说明Win32 GUI工作正常。\n\n点击确定继续通知测试。"),
            &HSTRING::from("Fluxor 通知测试"),
            MB_OK | MB_ICONINFORMATION
        );
        println!("MessageBox 结果: {:?}", result);
    }

    // 4. 尝试创建Toast通知
    println!("\n🍞 创建Toast通知...");

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

    // 方法2: 使用默认通知器
    match ToastNotificationManager::CreateToastNotifier() {
        Ok(notifier) => {
            println!("✅ 默认通知器创建成功");
            match notifier.Show(&toast) {
                Ok(_) => println!("🎉 通知显示成功! (默认)"),
                Err(e) => println!("❌ 通知显示失败: {:?}", e),
            }
        }
        Err(e) => println!("❌ 默认通知器创建失败: {:?}", e),
    }

    println!("\n⏰ 等待通知显示...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    // 6. 检查通知历史
    println!("\n📜 检查通知历史...");
    match ToastNotificationManager::History() {
        Ok(history) => {
            println!("✅ 通知历史获取成功");
            // 尝试获取历史数量
            match history.GetHistory() {
                Ok(notifications) => {
                    let count = notifications.Size().unwrap_or(0);
                    println!("📊 历史通知数量: {}", count);
                }
                Err(e) => println!("❌ 获取历史通知失败: {:?}", e),
            }
        }
        Err(e) => println!("❌ 通知历史获取失败: {:?}", e),
    }

    println!("\n🔧 最终诊断建议:");
    println!("1. 请检查 Windows 10/11 通知设置:");
    println!("   - 设置 > 系统 > 通知和操作");
    println!("   - 确保 '获取来自应用和其他发送者的通知' 已启用");

    println!("2. 检查专注助手:");
    println!("   - 点击右下角通知图标");
    println!("   - 确保没有开启勿扰模式");

    println!("3. 检查通知在操作中心:");
    println!("   - 按 Win + A 打开操作中心");
    println!("   - 查看是否有通知");

    println!("4. 重启通知服务:");
    println!("   - Win + R -> services.msc");
    println!("   - 找到 'Windows Push Notifications User Service'");
    println!("   - 重启服务");

    Ok(())
}
