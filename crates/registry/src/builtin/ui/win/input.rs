use super::*;

pub async fn ui_wait_impl(params: Value, ctx: &mut ExecutionContext) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let ms = map
        .get("ms")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("ms".into()))?
        .max(0) as u64;
    ctx.add_ui_settle_ms(ms).map_err(ActionError::execution)?;
    tokio::time::sleep(Duration::from_millis(ms)).await;
    Ok(Value::Bool(true))
}

fn mouse_button_flags(button: &str) -> Result<(u32, u32), ActionError> {
    match button.trim().to_ascii_lowercase().as_str() {
        "left" | "" => Ok((MOUSEEVENTF_LEFTDOWN.0, MOUSEEVENTF_LEFTUP.0)),
        "right" => Ok((MOUSEEVENTF_RIGHTDOWN.0, MOUSEEVENTF_RIGHTUP.0)),
        "middle" => Ok((MOUSEEVENTF_MIDDLEDOWN.0, MOUSEEVENTF_MIDDLEUP.0)),
        other => Err(ActionError::InvalidParams(format!(
            "不支持的 button: {other}（left|right|middle）"
        ))),
    }
}

fn mouse_click_at(x: i32, y: i32, button: &str, clicks: i64) -> Result<(), ActionError> {
    let (down_f, up_f) = mouse_button_flags(button)?;
    let clicks = clicks.clamp(1, 10) as usize;
    unsafe {
        SetCursorPos(x, y)
            .map_err(|e| ActionError::execution(format!("SetCursorPos 失败: {e}")))?;
    }
    for _ in 0..clicks {
        let down = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(down_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(up_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
        }
    }
    Ok(())
}

pub async fn ui_click_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let x = map
        .get("x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("x".into()))? as i32;
    let y = map
        .get("y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("y".into()))? as i32;
    let button = opt_str_map(map, "button").unwrap_or_else(|| "left".into());
    let clicks = opt_i64(map, "clicks", 1);
    tokio::task::spawn_blocking(move || {
        mouse_click_at(x, y, &button, clicks)?;
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.click 失败: {e}")))?
}

fn opt_str_map(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str().map(|s| s.to_string()))
}

pub async fn ui_scroll_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let dy = opt_i64(map, "dy", 0);
    let dx = opt_i64(map, "dx", 0);
    if dy == 0 && dx == 0 {
        return Err(ActionError::InvalidParams("需要 dy 或 dx".into()));
    }
    let x = map.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
    let y = map.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);
    tokio::task::spawn_blocking(move || {
        if let (Some(x), Some(y)) = (x, y) {
            unsafe {
                SetCursorPos(x, y)
                    .map_err(|e| ActionError::execution(format!("SetCursorPos 失败: {e}")))?;
            }
        }
        let mut inputs = Vec::new();
        if dy != 0 {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dy as i16 as u16) as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        if dx != 0 {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dx as i16 as u16) as u32,
                        dwFlags: MOUSEEVENTF_HWHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        unsafe {
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.scroll 失败: {e}")))?
}

pub async fn ui_drag_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let from_x = map
        .get("from_x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("from_x".into()))? as i32;
    let from_y = map
        .get("from_y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("from_y".into()))? as i32;
    let to_x = map
        .get("to_x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("to_x".into()))? as i32;
    let to_y = map
        .get("to_y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("to_y".into()))? as i32;
    let steps = opt_i64(map, "steps", 12).clamp(1, 100) as i32;
    let button = opt_str_map(map, "button").unwrap_or_else(|| "left".into());
    let (down_f, up_f) = mouse_button_flags(&button)?;
    tokio::task::spawn_blocking(move || {
        unsafe {
            SetCursorPos(from_x, from_y)
                .map_err(|e| ActionError::execution(format!("SetCursorPos 失败: {e}")))?;
        }
        let down = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(down_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
        }
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let x = from_x as f64 + (to_x - from_x) as f64 * t;
            let y = from_y as f64 + (to_y - from_y) as f64 * t;
            unsafe {
                let _ = SetCursorPos(x as i32, y as i32);
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        let up = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(up_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.drag 失败: {e}")))?
}

fn send_unicode_char(ch: char) {
    let code = ch as u16;
    let down = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: code,
                dwFlags: KEYEVENTF_UNICODE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: code,
                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        let _ = SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
    }
}

fn send_vk(vk: u16, key_up: bool) {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

fn vk_from_token(tok: &str) -> Option<u16> {
    match tok.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(0x11),
        "alt" => Some(0x12),
        "shift" => Some(0x10),
        "win" | "meta" => Some(0x5B),
        "enter" | "return" => Some(0x0D),
        "tab" => Some(0x09),
        "esc" | "escape" => Some(0x1B),
        "space" => Some(0x20),
        "backspace" => Some(0x08),
        "delete" | "del" => Some(0x2E),
        "up" => Some(0x26),
        "down" => Some(0x28),
        "left" => Some(0x25),
        "right" => Some(0x27),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" | "pgup" => Some(0x21),
        "pagedown" | "pgdn" => Some(0x22),
        "insert" | "ins" => Some(0x2D),
        "f1" => Some(0x70),
        "f2" => Some(0x71),
        "f3" => Some(0x72),
        "f4" => Some(0x73),
        "f5" => Some(0x74),
        s if s.len() == 1 => {
            let c = s.chars().next()?.to_ascii_uppercase();
            if c.is_ascii_alphanumeric() {
                Some(c as u16)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub async fn ui_type_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    tokio::task::spawn_blocking(move || {
        for ch in text.chars() {
            send_unicode_char(ch);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.type 失败: {e}")))?
}

pub async fn ui_key_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let keys = require_str(map, "keys")?;
    tokio::task::spawn_blocking(move || {
        let parts: Vec<&str> = keys
            .split('+')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            return Err(ActionError::InvalidParams("keys 为空".into()));
        }
        let mut vks = Vec::new();
        for p in &parts {
            let vk = vk_from_token(p)
                .ok_or_else(|| ActionError::InvalidParams(format!("不支持的 keys 片段: {p}")))?;
            vks.push(vk);
        }
        for vk in &vks {
            send_vk(*vk, false);
        }
        for vk in vks.iter().rev() {
            send_vk(*vk, true);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.key 失败: {e}")))?
}
