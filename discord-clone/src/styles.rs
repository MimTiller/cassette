//! Stylesheets for the elements the app draws most (message rows, …).
//! Declared once with `stylesheet!` instead of building `StyleRules` per
//! render: the sheet is resolved and cached once, hover is a CSS state
//! rather than a signal, and a release build can premint it to CSS.
//! Colours are theme roles (`col(theme::ROLE)`), so theme swaps still apply.

use runtime_core::stylesheet;
use runtime_core::{AlignItems, Color, FlexDirection, FlexWrap, FontWeight, JustifyContent, Length, ObjectFit, Position, Tokenized};

use crate::style_helpers::col;
use crate::theme;

stylesheet! {
    pub MessageRowBox<()> {
        base(_t) {
            padding_top: 8.0,
            padding_bottom: 2.0,
            padding_horizontal: 16.0,
            gap: 10.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::FlexStart,
        }
        state hovered(_t) {
            background: col(theme::BG_HOVER),
        }
    }
}

stylesheet! {
    /// A grouped follow-up message: just the body, under the name column.
    pub MessageGroupedBox<()> {
        base(_t) {
            padding_top: 3.0,
            padding_bottom: 2.0,
            padding_left: 66.0,
            padding_right: 16.0,
        }
        state hovered(_t) {
            background: col(theme::BG_HOVER),
        }
    }
}

stylesheet! {
    /// A 40px round avatar (image or initial); the caller sets the fill.
    pub Avatar40<()> {
        base(_t) {
            width: 40.0,
            height: 40.0,
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: 20.0,
            object_fit: ObjectFit::Cover,
        }
        override background: Tokenized<Color>
    }
}

stylesheet! {
    pub AvatarInitial<()> {
        base(_t) {
            font_size: 15.0,
            font_weight: FontWeight::Bold,
            color: col(theme::TEXT_STRONG),
        }
    }
}

stylesheet! {
    /// The column beside the avatar: header line, body, extras.
    pub MessageStack<()> {
        base(_t) {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            gap: 2.0,
        }
    }
}

stylesheet! {
    /// The header line: name, then time.
    pub MessageMeta<()> {
        base(_t) {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Baseline,
            gap: 6.0,
        }
    }
}

stylesheet! {
    /// The author name; the caller sets the role colour.
    pub AuthorName<()> {
        base(_t) {
            font_size: 15.0,
            font_weight: FontWeight::SemiBold,
            color: col(theme::TEXT_STRONG),
        }
        override color: Tokenized<Color>
    }
}

stylesheet! {
    /// The body's inline flow: text runs, emoji, mention pills, spoilers.
    pub MessageBodyFlow<()> {
        base(_t) {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Baseline,
            flex_grow: 1.0,
        }
    }
}

stylesheet! {
    /// One run of body text; emoji runs are bigger, and bigger still when
    /// the message is nothing but emoji.
    pub BodyText<()> {
        base(_t) {
            font_size: 15.0,
            color: col(theme::TEXT_STRONG),
        }
        variant size {
            #[default]
            text(_t) {}
            emoji(_t) { font_size: 22.0 }
            emoji_only(_t) { font_size: 30.0 }
        }
    }
}

stylesheet! {
    /// A mention pill; the caller sets the tint (a role's own colour).
    pub MentionPill<()> {
        base(_t) {
            padding_horizontal: 2.0,
            border_radius: 3.0,
            background: col(theme::MENTION_BG),
        }
        override background: Tokenized<Color>
    }
}

stylesheet! {
    pub MentionPillText<()> {
        base(_t) {
            font_size: 15.0,
            font_weight: FontWeight::SemiBold,
            color: col(theme::MENTION_TEXT),
        }
        override color: Tokenized<Color>
    }
}

stylesheet! {
    /// A message that mentions the signed-in user.
    pub MentionHighlight<()> {
        base(_t) {
            background: col(theme::MENTION_ROW),
            border_left_width: 2.0,
            border_left_color: col(theme::WARNING),
        }
    }
}

stylesheet! {
    pub DateDivider<()> {
        base(_t) {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding_horizontal: 16.0,
            margin_top: 16.0,
            margin_bottom: 8.0,
        }
    }
}

stylesheet! {
    pub DateDividerLabel<()> {
        base(_t) {
            font_size: 12.0,
            font_weight: FontWeight::SemiBold,
            color: col(theme::TEXT_MUTED),
            margin_bottom: 4.0,
        }
    }
}

stylesheet! {
    /// The divider's 1px rule across the message column.
    pub DateDividerRule<()> {
        base(_t) {
            height: 0.0,
            width: Length::pct(100.0),
            border_bottom_width: 1.0,
            border_bottom_color: col(theme::BG_TERTIARY),
        }
    }
}

stylesheet! {
    /// The strip of reaction pills under a message, aligned with the body.
    pub ReactionStrip<()> {
        base(_t) {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            gap: 4.0,
            padding_top: 2.0,
            padding_left: 66.0,
            padding_right: 16.0,
        }
    }
}

stylesheet! {
    /// One reaction: emoji and count; tinted when the signed-in user reacted.
    pub ReactionPill<()> {
        base(_t) {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            gap: 5.0,
            padding_horizontal: 8.0,
            padding_vertical: 3.0,
            height: 22.0,
            border_radius: 11.0,
            border_width: 1.0,
            background: col(theme::BG_PRIMARY),
            border_color: col(theme::BUTTON_SECONDARY),
        }
        variant reacted {
            #[default]
            off(_t) {}
            on(_t) {
                background: col(theme::BG_ACCENT_SUBTLE),
                border_color: col(theme::ACCENT),
            }
        }
        state hovered(_t) {
            border_color: col(theme::INTERACTIVE),
        }
    }
}

stylesheet! {
    pub ReactionEmoji<()> {
        base(_t) {
            font_size: 14.0,
        }
    }
}

stylesheet! {
    pub ReactionCount<()> {
        base(_t) {
            font_size: 12.0,
            font_weight: FontWeight::SemiBold,
            color: col(theme::TEXT_STRONG),
        }
    }
}

stylesheet! {
    /// The round "add reaction" button at the end of the strip.
    pub AddReactionButton<()> {
        base(_t) {
            position: Position::Relative,
            width: 26.0,
            height: 26.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            background: col(theme::BG_INPUT),
            border_radius: 13.0,
        }
        state hovered(_t) {
            background: col(theme::BG_HOVER),
        }
    }
}

stylesheet! {
    /// A flexible gap that pushes what follows to the end of a row.
    pub Spacer<()> {
        base(_t) {
            flex_grow: 1.0,
        }
    }
}

stylesheet! {
    /// A text button; `kind` picks its colour.
    pub ButtonBox<()> {
        base(_t) {
            height: 36.0,
            padding_horizontal: 16.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border_radius: 4.0,
            background: col(theme::ACCENT),
        }
        variant kind {
            #[default]
            primary(_t) {}
            secondary(_t) { background: col(theme::BUTTON_SECONDARY) }
            success(_t) { background: col(theme::SUCCESS_STRONG) }
            danger(_t) { background: col(theme::DANGER) }
        }
        state hovered(_t) {
            opacity: 0.88,
        }
        state pressed(_t) {
            opacity: 0.75,
        }
    }
}

stylesheet! {
    pub ButtonLabel<()> {
        base(_t) {
            font_size: 14.0,
            font_weight: FontWeight::SemiBold,
            color: col(theme::TEXT_ON_ACCENT),
        }
    }
}
