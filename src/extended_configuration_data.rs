#[derive(Clone,Debug)]
pub struct ExtendedConfigurationData{
    pub chroma_format: u64,
    pub separate_color_plane: Option<bool>,
    pub bit_depth_luma_minus_8: u64,
    pub bit_depth_chroma_minus_8: u64,
    pub qp_prime_y_zero_transform_bypass: bool,
}