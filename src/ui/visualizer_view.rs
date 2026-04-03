use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::ui::theme::Theme;

/// Unicode block characters for bar rendering, from empty to full.
const BAR_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render the audio visualizer with frequency bars.
/// `bar_heights` should contain values in 0.0..=1.0 for each bar.
pub fn render_visualizer(
    frame: &mut Frame,
    area: Rect,
    bar_heights: &[f64],
    theme: &Theme,
) {
    let block = Block::default()
        .title(" Visualizer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 || bar_heights.is_empty() {
        return;
    }

    let num_bars = bar_heights.len();
    let available_width = inner.width as usize;
    let height = inner.height as usize;

    // Calculate bar width and spacing
    // Each bar takes at least 1 column, plus 1 column gap between bars
    let bar_width = ((available_width + 1) / (num_bars + 1)).max(1);
    let total_bar_width = num_bars * bar_width + (num_bars.saturating_sub(1));

    // Center bars horizontally
    let x_offset = if total_bar_width < available_width {
        (available_width - total_bar_width) / 2
    } else {
        0
    };

    let buf = frame.buffer_mut();

    for (i, &bar_val) in bar_heights.iter().enumerate() {
        let bar_x = inner.x + (x_offset + i * (bar_width + 1)) as u16;
        if bar_x >= inner.x + inner.width {
            break;
        }

        let val = bar_val.clamp(0.0, 1.0);
        let color = bar_color(val, &theme.visualizer_colors);

        // How many full rows + fractional part
        let bar_total = val * height as f64;
        let full_rows = bar_total as usize;
        let fraction = bar_total - full_rows as f64;

        // Render from bottom to top
        for row in 0..height {
            let y = inner.y + (height - 1 - row) as u16;
            if y < inner.y || y >= inner.y + inner.height {
                continue;
            }

            let ch = if row < full_rows {
                BAR_CHARS[8] // full block
            } else if row == full_rows {
                // Fractional block
                let idx = (fraction * 8.0) as usize;
                BAR_CHARS[idx.min(8)]
            } else {
                ' '
            };

            for bw in 0..bar_width {
                let x = bar_x + bw as u16;
                if x < inner.x + inner.width {
                    let cell = &mut buf[(x, y)];
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(color));
                }
            }
        }
    }
}

/// Pick a color from the gradient based on bar height (0.0 = low, 1.0 = high).
fn bar_color(val: f64, colors: &[Color]) -> Color {
    if colors.is_empty() {
        return Color::Green;
    }
    if colors.len() == 1 {
        return colors[0];
    }
    let idx = (val * (colors.len() - 1) as f64).round() as usize;
    colors[idx.min(colors.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_color_single() {
        assert_eq!(bar_color(0.5, &[Color::Red]), Color::Red);
    }

    #[test]
    fn test_bar_color_gradient() {
        let colors = vec![Color::Green, Color::Yellow, Color::Red];
        assert_eq!(bar_color(0.0, &colors), Color::Green);
        assert_eq!(bar_color(0.5, &colors), Color::Yellow);
        assert_eq!(bar_color(1.0, &colors), Color::Red);
    }

    #[test]
    fn test_bar_color_empty() {
        assert_eq!(bar_color(0.5, &[]), Color::Green);
    }

    #[test]
    fn test_bar_chars_length() {
        assert_eq!(BAR_CHARS.len(), 9);
        assert_eq!(BAR_CHARS[0], ' ');
        assert_eq!(BAR_CHARS[8], '█');
    }
}
