#[derive(Clone, Debug)]
pub struct ExtendedConfigurationData {
    pub chroma_format: u64,
    pub separate_color_plane: Option<bool>,
    pub bit_depth_luma_minus_8: u64,
    pub bit_depth_chroma_minus_8: u64,
    pub qp_prime_y_zero_transform_bypass: bool,
    pub seq_scaling_matrix_present: bool,
    pub seq_scaling_list_4x4: Option<Vec<Vec<i64>>>,
    pub seq_scaling_list_4x4_use_default: Option<Vec<bool>>,
    pub seq_scaling_list_8x8: Option<Vec<Vec<i64>>>,
    pub seq_scaling_list_8x8_use_default: Option<Vec<bool>>,
}
