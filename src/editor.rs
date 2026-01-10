use crate::os::{Os, OsKey};

fn edit_banner(os: &impl Os) {
    os.clear_text();
    println!("--- Redox Bootloader Environment Editor ---");
    println!("ENTER twice to boot. UP/DOWN to edit lines.");
    println!("-------------------------------------------");
}

pub fn edit_env(os: &impl Os, env_ptr: *mut u8, env_size: &mut usize, max_size: usize) {
    edit_banner(os);

    let env_slice = unsafe { core::slice::from_raw_parts_mut(env_ptr, max_size) };
    // counting at line index
    let mut cursor = 0xFFF;
    // position at current line, not including LF
    let mut cursor_start = 0;
    let mut cursor_end = 0;
    let original_size = *env_size;

    loop {
        os.set_text_position(0, 4);

        let mut iline = 0;
        os.set_text_highlight(cursor == 0);
        for i in 0..*env_size {
            let c = env_slice[i] as char;
            if c == '\n' {
                print!(" \n");
                iline += 1;
                os.set_text_highlight(iline == cursor);
                if iline == cursor {
                    cursor_start = i + 1;
                }
            } else {
                print!("{}", c);
            }
            if iline == cursor {
                cursor_end = i + 1;
            }
        }
        if cursor > iline {
            cursor = iline;
            // update cursors, should never hang
            continue;
        }
        print!(" ");
        os.set_text_highlight(false);

        match os.get_key() {
            OsKey::Enter => {
                if cursor_start == cursor_end {
                    // blank line to boot
                    break;
                }

                if *env_size < max_size - 1 {
                    if *env_size == max_size {
                        continue;
                    }
                    for i in (cursor_end..*env_size).rev() {
                        env_slice[i + 1] = env_slice[i];
                    }
                    env_slice[cursor_end] = b'\n';
                    *env_size += 1;
                    cursor += 1;
                    edit_banner(os);
                }
            }
            OsKey::Backspace => {
                if cursor_end == 0 {
                    continue;
                }
                if cursor_start == cursor_end && iline > 0 {
                    iline -= 1;
                }
                for i in cursor_end..*env_size {
                    env_slice[i - 1] = env_slice[i];
                }
                *env_size -= 1;
                edit_banner(os);
            }
            OsKey::Up => {
                if cursor > 0 {
                    cursor -= 1;
                }
            }
            OsKey::Down => {
                cursor += 1;
            }
            OsKey::Char(c) => {
                if *env_size == max_size {
                    continue;
                }
                for i in (cursor_end..*env_size).rev() {
                    env_slice[i + 1] = env_slice[i];
                }
                env_slice[cursor_end] = c.to_ascii_uppercase() as u8;
                *env_size += 1;
            }
            _ => (),
        }
    }

    if *env_size == 0 || env_slice[*env_size - 1] != b'\n' {
        if *env_size < max_size {
            env_slice[*env_size] = b'\n';
            *env_size += 1;
        }
    }

    if *env_size < original_size {
        for i in (*env_size..original_size).rev() {
            env_slice[i] = 0;
        }
    }

    os.set_text_highlight(false);
    println!("\nBooting...");
}
