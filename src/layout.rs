// Color scheme, use rainbow mode
// base: #ed6a5a, #18314f, #ffee93, #62bbc1, #b744b8
// #ed6a5a #3b0d07 #76190d #b02614 #e6361f #ed6a5a #f0897b #f4a69c #f8c4bd #fbe1de
// #18314f #050a10 #0a131f #0e1d2f #13273e #18314f #2b578c #447ec5 #82a9d9 #c1d4ec
// #ffee93 #514400 #a18900 #f2cd00 #ffe343 #ffee93 #fff2a9 #fff5bf #fff9d4 #fffcea
// #62bbc1 #11282a #215053 #32787d #42a0a6 #62bbc1 #82c8cd #a1d6d9 #c0e3e6 #e0f1f2
// #b744b8 #240d24 #481b49 #6c286d #903692 #b744b8 #c567c7 #d48dd5 #e2b3e3 #f1d9f1


pub mod header {
    pub const MARGIN: f32 = 0.1;
}

pub mod colors {
    use std::collections::HashMap;

    use egui::Color32;
        
    pub const OBSERVABLES: Color32 = Color32::from_rgb(0x62, 0xbb, 0xc1);
    pub const LAUNCHCONTROL: Color32 = Color32::from_rgb(0xed, 0x6a, 0x52);

    pub fn muted(color: Color32) -> Color32 {
        let muted_colors = HashMap::from([
        (OBSERVABLES, Color32::from_rgb(0x32, 0x78, 0x7d)),    // #62bbc1 -> #32787d
         (LAUNCHCONTROL, Color32::from_rgb(0xb0, 0x26, 0x14))]); // #ed6a5a -> #b02614
        muted_colors[&color]
    }
    
}

