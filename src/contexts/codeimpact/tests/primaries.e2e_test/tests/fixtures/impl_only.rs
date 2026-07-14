struct Widget {
    visible: bool,
}

impl Widget {
    fn render(&self) -> u32 {
        if self.is_visible() {
            1
        } else {
            0
        }
    }

    fn is_visible(&self) -> bool {
        self.visible
    }
}
