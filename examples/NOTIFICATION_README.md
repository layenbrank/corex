# Fluxor 通知系统

Fluxor 的通知系统已经过全面优化，现在支持多种通知创建方式，包括从本地 XML 文件加载模板。

## 功能特性

✅ **链式构建** - 支持流畅的方法链调用  
✅ **XML 模板** - 支持从本地 XML 文件加载通知模板  
✅ **配置管理** - 支持从 JSON 配置文件加载设置  
✅ **占位符替换** - 支持动态替换 XML 模板中的占位符  
✅ **多种通知样式** - 基础、高级、任务完成等模板  
✅ **错误处理** - 完善的错误处理和用户反馈

## 快速开始

### 1. 简单通知

```rust
use crate::utils::notification::Notification;

// 最简单的方式
Notification::quick_show("标题", "内容")?;

// 链式构建
Notification::new()
    .title("我的标题")
    .content("我的内容")
    .show()?;
```

### 2. 自定义配置

```rust
use crate::utils::notification::{Notification, NotificationConfig, ToastDuration};

let config = NotificationConfig {
    app_id: "com.myapp.custom".to_string(),
    icon_path: Some("path/to/icon.png".to_string()),
    sound_enabled: true,
    duration: ToastDuration::Long,
};

Notification::with_config(config)
    .title("自定义通知")
    .content("使用自定义配置")
    .show()?;
```

### 3. 从 XML 文件加载

```rust
// 直接从XML文件显示
Notification::show_from_xml_file("templates/notification_basic.xml")?;

// 或者加载后再自定义
Notification::new()
    .from_xml_file("templates/notification_advanced.xml")?
    .show()?;
```

### 4. 从配置文件加载设置

```rust
// 从配置文件加载设置并显示通知
Notification::show_with_config_file(
    "notification_config.json",
    "配置通知",
    "这是从配置文件加载的通知"
)?;
```

### 5. 使用占位符模板

```rust
use std::collections::HashMap;

let mut placeholders = HashMap::new();
placeholders.insert("file_count".to_string(), "42".to_string());
placeholders.insert("status".to_string(), "成功".to_string());

Notification::new()
    .from_xml_file("templates/notification_task_complete.xml")?
    .replace_placeholders(placeholders)
    .show()?;
```

## XML 模板

### 基础模板 (templates/notification_basic.xml)

```xml
<?xml version="1.0" encoding="utf-8"?>
<toast>
    <visual>
        <binding template="ToastGeneric">
            <text id="1">Fluxor 基础通知</text>
            <text id="2">这是一个来自XML文件的基础通知模板</text>
        </binding>
    </visual>
    <audio silent="false"/>
</toast>
```

### 高级模板 (templates/notification_advanced.xml)

```xml
<?xml version="1.0" encoding="utf-8"?>
<toast duration="long">
    <visual>
        <binding template="ToastGeneric">
            <image placement="appLogoOverride" src="file:///C:/Windows/System32/SecurityAndMaintenance.png"/>
            <text id="1">Fluxor 高级通知</text>
            <text id="2">这是一个带图标和长时间显示的高级通知</text>
            <text id="3">支持更多自定义选项</text>
        </binding>
    </visual>
    <actions>
        <action content="确定" arguments="ok"/>
        <action content="取消" arguments="cancel"/>
    </actions>
    <audio src="ms-winsoundevent:Notification.Reminder"/>
</toast>
```

### 任务完成模板 (templates/notification_task_complete.xml)

支持占位符 `{file_count}` 的动态替换：

```xml
<?xml version="1.0" encoding="utf-8"?>
<toast scenario="alarm">
    <visual>
        <binding template="ToastGeneric">
            <text id="1">Fluxor 任务提醒</text>
            <text id="2">您的文件操作已完成</text>
            <text id="3">状态: 成功处理 {file_count} 个文件</text>
            <progress title="处理进度" status="完成" value="1"/>
        </binding>
    </visual>
    <actions>
        <action content="查看结果" arguments="view_results"/>
        <action content="打开目录" arguments="open_folder"/>
        <action content="关闭" arguments="dismiss"/>
    </actions>
    <audio src="ms-winsoundevent:Notification.Default" loop="false"/>
</toast>
```

## 配置文件

### notification_config.json

```json
{
  "notification": {
    "app_id": "com.fluxor.corex",
    "sound_enabled": true,
    "duration": "short",
    "icon_path": null,
    "templates": {
      "basic": "templates/notification_basic.xml",
      "advanced": "templates/notification_advanced.xml",
      "task_complete": "templates/notification_task_complete.xml"
    },
    "auto_notify": {
      "file_operations": true,
      "task_completion": true,
      "error_alerts": true
    }
  }
}
```

## 代码优化亮点

### 1. 结构化设计

- 分离了通知内容和配置
- 支持链式方法调用
- 清晰的错误处理

### 2. 灵活的模板系统

- 支持 XML 文件加载
- 占位符动态替换
- 多种预设模板

### 3. 配置管理

- JSON 配置文件支持
- 序列化/反序列化
- 默认值提供

### 4. 错误处理

- 使用 `anyhow` 进行错误链追踪
- 用户友好的错误消息
- 优雅的失败处理

### 5. Windows 集成

- 正确的 AppUserModelID 设置
- Toast 通知完整支持
- 音频和视觉效果控制

## 使用建议

1. **开发阶段**: 使用简单的 `quick_show` 方法快速测试
2. **生产环境**: 使用配置文件管理通知设置
3. **定制需求**: 创建自定义 XML 模板并使用占位符
4. **批量操作**: 使用任务完成模板提供进度反馈

这个优化后的通知系统为 Fluxor 提供了强大而灵活的用户反馈机制，大大提升了用户体验。
