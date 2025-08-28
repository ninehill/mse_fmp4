use std::io::Write;

use crate::Result;

#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub struct HvcDecoderConfigurationRecord {
    pub general_profile_space: u8,
    pub general_tier_flag: u8,
    pub general_profile_idc: u8,
    pub general_profile_compatibility_flags: u32,
    pub general_constraint_indicator_flags: u64,
    pub general_level_idc: u8,
    pub min_spatial_segmentation_idc: u16,
    pub parallelism_type: u8,
    pub chroma_format_idc: u8,
    pub bit_depth_luma_minus8: u8,
    pub bit_depth_chroma_minus8: u8,
    pub avg_frame_rate: u16,
    pub constant_frame_rate: u8,
    pub num_temporal_layers: u8,
    pub temporal_id_nested: u8,
    pub length_size_minus_one: u8,
    pub sps_data: Vec<u8>,
    pub pps_data: Vec<u8>,
    pub vps_data: Vec<u8>,
}

impl HvcDecoderConfigurationRecord {
    pub fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        write_u8!(writer, 1); // configuration_version

        write_u8!(
            writer,
            ((self.general_profile_space << 6) & 0b1100_0000)
                | ((self.general_tier_flag << 5) & 0b0010_0000)
                | (self.general_profile_idc & 0b0001_1111)
        );
        write_u32!(writer, self.general_profile_compatibility_flags);
        write_all!(
            writer,
            &self.general_constraint_indicator_flags.to_be_bytes()[0..6]
        );
        write_u8!(writer, self.general_level_idc);
        write_u16!(writer, self.min_spatial_segmentation_idc & 0x0FFF);
        write_u8!(writer, self.parallelism_type & 0b11);
        write_u8!(writer, self.chroma_format_idc & 0b11);
        write_u8!(writer, self.bit_depth_luma_minus8 & 0b111);
        write_u8!(writer, self.bit_depth_chroma_minus8 & 0b1111);
        write_u16!(writer, self.avg_frame_rate);
        write_u8!(
            writer,
            ((self.constant_frame_rate & 0b11) << 6)
                | ((self.num_temporal_layers & 0b111) << 3)
                | (self.temporal_id_nested << 2)
                | (self.length_size_minus_one & 0b11)
        );
        write_u8!(writer, 0x2); // num_of_arrays

        // vps data
        write_u8!(writer, 32 & 0x3F); // NAL unit type
        write_u16!(writer, 1); // num_of_vps
        write_u16!(writer, self.vps_data.len() as u16);
        write_all!(writer, &self.vps_data);

        // sps data
        write_u8!(writer, 33 & 0x3F); // NAL unit type
        write_u16!(writer, 1); // num_of_sps
        write_u16!(writer, self.sps_data.len() as u16);
        write_all!(writer, &self.sps_data);

        // pps pps_data
        write_u8!(writer, 34 & 0x3F); // NAL unit type
        write_u16!(writer, 1); // num_of_pps
        write_u16!(writer, self.pps_data.len() as u16);
        write_all!(writer, &self.pps_data);

        Ok(())
    }
}
