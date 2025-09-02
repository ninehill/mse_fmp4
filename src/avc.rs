//! AVC (H.264) related constituent elements.
use crate::extended_configuration_data::{self, ExtendedConfigurationData};
use crate::io::{AvcBitReader, AvcBitWriter};
use crate::{ErrorKind, Result};
use byteorder::ReadBytesExt;
use core::panic;
use std::io::{Read, Write};

const DEFAULT_4X4_INTRA_SCALING_LIST: [u8; 16] = [
    6, 13, 13, 20, 20, 20, 28, 28, 28, 28, 32, 32, 32, 32, 37, 37,
];
const DEFAULT_4X4_INTER_SCALING_LIST: [u8; 16] = [
    10, 14, 14, 20, 20, 20, 24, 24, 24, 24, 27, 27, 27, 30, 30, 34,
];
const DEFAULT_8X8_INTRA_SCALING_LIST: [u8; 64] = [
    6, 10, 10, 13, 11, 13, 16, 16, 16, 16, 18, 18, 18, 18, 18, 23, 23, 23, 23, 23, 23, 25, 25, 25,
    25, 25, 25, 25, 27, 27, 27, 27, 27, 27, 27, 27, 29, 29, 29, 29, 29, 29, 29, 31, 31, 31, 31, 31,
    31, 33, 33, 33, 33, 33, 36, 36, 36, 36, 38, 38, 38, 40, 40, 42,
];
const DEFAULT_8X8_INTER_SCALING_LIST: [u8; 64] = [
    9, 13, 13, 15, 13, 15, 17, 17, 17, 17, 19, 19, 19, 19, 19, 21, 21, 21, 21, 21, 21, 22, 22, 22,
    22, 22, 22, 22, 24, 24, 24, 24, 24, 24, 24, 24, 25, 25, 25, 25, 25, 25, 25, 27, 27, 27, 27, 27,
    27, 28, 28, 28, 28, 28, 30, 30, 30, 30, 32, 32, 32, 33, 33, 35,
];

/// AVC decoder configuration record.
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub struct AvcDecoderConfigurationRecord {
    pub profile_idc: u8,
    pub constraint_set_flag: u8,
    pub level_idc: u8,
    pub sequence_parameter_set: Vec<u8>,
    pub picture_parameter_set: Vec<u8>,
    pub extended_configuration_data: Option<ExtendedConfigurationData>,
}
impl AvcDecoderConfigurationRecord {
    pub fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        write_u8!(writer, 1); // configuration_version
        write_u8!(writer, self.profile_idc);
        write_u8!(writer, self.constraint_set_flag);
        write_u8!(writer, self.level_idc);
        write_u8!(writer, 0b1111_1100 | 0b0000_0011); // reserved and length_size_minus_one

        write_u8!(writer, 0b1110_0000 | 0b0000_0001); // reserved and num_of_sequence_parameter_set_ext
        write_u16!(writer, self.sequence_parameter_set.len() as u16);
        write_all!(writer, &self.sequence_parameter_set);

        write_u8!(writer, 0b0000_0001); // num_of_picture_parameter_set_ext
        write_u16!(writer, self.picture_parameter_set.len() as u16);
        write_all!(writer, &self.picture_parameter_set);

        match self.profile_idc {
            100 | 110 | 122 | 144 => {
                if self.extended_configuration_data.is_none() {
                    track_panic!(
                        ErrorKind::Unsupported,
                        "Profile IDC is {}, but missing extended configuration data",
                        self.profile_idc
                    );
                }
                let extended_configuration_data =
                    self.extended_configuration_data.as_ref().unwrap();

                let mut bit_writer = AvcBitWriter::new(writer);

                bit_writer.write_ue(extended_configuration_data.chroma_format)?;
                if extended_configuration_data.chroma_format == 3 {
                    let separate_color_plane = extended_configuration_data
                        .separate_color_plane
                        .unwrap_or_else(|| {
                            panic!("Must have optional flag set when chroma format is YUV444")
                        });
                    bit_writer.write_bool(separate_color_plane)?;
                }

                bit_writer.write_ue(extended_configuration_data.bit_depth_luma_minus_8)?;
                bit_writer.write_ue(extended_configuration_data.bit_depth_chroma_minus_8)?;
                bit_writer
                    .write_bool(extended_configuration_data.qp_prime_y_zero_transform_bypass)?;
                bit_writer.write_bool(false)?; //False for scaling matrix
                bit_writer.flush()?;
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct SpsSummary {
    pub profile_idc: u8,
    pub constraint_set_flag: u8,
    pub level_idc: u8,
    pic_width_in_mbs_minus_1: u64,
    pic_height_in_map_units_minus_1: u64,
    frame_mbs_only_flag: u8,
    frame_crop_left_offset: u64,
    frame_crop_right_offset: u64,
    frame_crop_top_offset: u64,
    frame_crop_bottom_offset: u64,
    pub extended_configuration_data: Option<ExtendedConfigurationData>,
}
impl SpsSummary {
    pub fn width(&self) -> usize {
        (self.pic_width_in_mbs_minus_1 as usize + 1) * 16
            - (self.frame_crop_right_offset as usize * 2)
            - (self.frame_crop_left_offset as usize * 2)
    }

    pub fn height(&self) -> usize {
        (2 - self.frame_mbs_only_flag as usize)
            * ((self.pic_height_in_map_units_minus_1 as usize + 1) * 16)
            - (self.frame_crop_bottom_offset as usize * 2)
            - (self.frame_crop_top_offset as usize * 2)
    }

    fn default_scaling_list_for_index(index: usize) -> Vec<i64> {
        if index < 3 {
            DEFAULT_4X4_INTRA_SCALING_LIST
                .iter()
                .map(|&v| v as i64)
                .collect()
        } else if index < 6 {
            DEFAULT_4X4_INTER_SCALING_LIST
                .iter()
                .map(|&v| v as i64)
                .collect()
        } else if index % 2 == 0 {
            DEFAULT_8X8_INTRA_SCALING_LIST
                .iter()
                .map(|&v| v as i64)
                .collect()
        } else {
            DEFAULT_8X8_INTER_SCALING_LIST
                .iter()
                .map(|&v| v as i64)
                .collect()
        }
    }

    pub fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let profile_idc = track_io!(reader.read_u8())?;
        let constraint_set_flag = track_io!(reader.read_u8())?;
        let level_idc = track_io!(reader.read_u8())?;

        let mut reader = AvcBitReader::new(reader);
        let _seq_parameter_set_id = track!(reader.read_ue())?;

        let mut extended_data = None;

        match profile_idc {
            100 | 110 | 122 | 144 => {
                //let chroma_format = track!(reader.read_byte())?;
                let chroma_format = track!(reader.read_ue())?;
                let separate_color_plane = if chroma_format == 3 {
                    //YUV 444
                    Some(true)
                } else {
                    None
                };
                let bit_depth_luma_minus_8 = track!(reader.read_ue())?;
                let bit_depth_chroma_minus_8 = track!(reader.read_ue())?;
                let qp_prime_y_zero_transform_bypass = track!(reader.read_bit())? == 1;
                let seq_scaling_matrix_present = track!(reader.read_bit())? == 1;

                let mut seq_scaling_list_4x4 = None;
                let mut seq_scaling_list_8x8 = None;
                let mut seq_scaling_list_4x4_use_default = None;
                let mut seq_scaling_list_8x8_use_default = None;

                if seq_scaling_matrix_present {
                    let entry_count = if chroma_format != 3 { 8 } else { 12 };
                    let mut sl_4x4 = vec![vec![0; 16]; entry_count];
                    let mut sl_8x8 = vec![vec![0; 64]; entry_count];
                    let mut sl_default_4x4_flag = vec![false; entry_count];
                    let mut sl_default_8x8_flag = vec![false; entry_count];

                    for i in 0..entry_count {
                        let scaling_list_present = track!(reader.read_bit())? == 1;
                        if scaling_list_present {
                            let mut last_scale = 8;
                            let mut next_scale = 8;
                            if i < 6 {
                                for j in 0..16 {
                                    if next_scale != 0 {
                                        let delta_scale = track!(reader.read_se())?;
                                        next_scale = (last_scale + delta_scale + 256) % 256;
                                        sl_default_4x4_flag[i] = j == 0 && next_scale == 0;
                                    }
                                    sl_4x4[i][j] = if next_scale == 0 {
                                        last_scale
                                    } else {
                                        next_scale
                                    };
                                    last_scale = sl_4x4[i][j];
                                }
                            } else {
                                for j in 0..64 {
                                    if next_scale != 0 {
                                        let delta_scale = track!(reader.read_se())?;
                                        next_scale = (last_scale + delta_scale + 256) % 256;
                                        sl_default_8x8_flag[i - 6] = j == 0 && next_scale == 0;
                                    }
                                    sl_8x8[i - 6][j] = if next_scale == 0 {
                                        last_scale
                                    } else {
                                        next_scale
                                    };
                                    last_scale = sl_8x8[i - 6][j];
                                }
                            }
                        } else {
                            // scaling list not present, use the fallback method A
                            match i {
                                0 | 3 | 6 | 7 => {
                                    sl_4x4[i] = Self::default_scaling_list_for_index(i);
                                    sl_default_4x4_flag[i] = true;
                                }
                                1 | 2 | 4 | 5 => {
                                    sl_4x4[i] = sl_4x4[i - 1].clone();
                                    sl_default_4x4_flag[i] = sl_default_4x4_flag[i - 1];
                                }
                                8..=11 => {
                                    sl_8x8[i - 6] = sl_8x8[i - 8].clone();
                                    sl_default_8x8_flag[i - 6] = sl_default_8x8_flag[i - 8];
                                }
                                _ => panic!("scaling list index out of range"),
                            }
                        }
                    }
                    seq_scaling_list_4x4 = Some(sl_4x4);
                    seq_scaling_list_8x8 = Some(sl_8x8);
                    seq_scaling_list_4x4_use_default = Some(sl_default_4x4_flag);
                    seq_scaling_list_8x8_use_default = Some(sl_default_8x8_flag);
                }

                extended_data = Some(ExtendedConfigurationData {
                    chroma_format,
                    separate_color_plane,
                    bit_depth_luma_minus_8,
                    bit_depth_chroma_minus_8,
                    qp_prime_y_zero_transform_bypass,
                    seq_scaling_matrix_present,
                    seq_scaling_list_4x4,
                    seq_scaling_list_4x4_use_default,
                    seq_scaling_list_8x8,
                    seq_scaling_list_8x8_use_default,
                })
            }
            _ => {}
        }

        let _log2_max_frame_num_minus4 = track!(reader.read_ue())?;
        let pic_order_cnt_type = track!(reader.read_ue())?;
        match pic_order_cnt_type {
            0 => {
                let _log2_max_pic_order_cnt_lsb_minus4 = track!(reader.read_ue())?;
            }
            1 => {
                let _delta_pic_order_always_zero_flag = track!(reader.read_bit())?;
                let _offset_for_non_ref_pic = track!(reader.read_ue())?;
                let _ffset_for_top_to_bottom_field = track!(reader.read_ue())?;
                let num_ref_frames_in_pic_order_cnt_cycle = track!(reader.read_ue())?;
                for _ in 0..num_ref_frames_in_pic_order_cnt_cycle {
                    let _offset_for_ref_frame = track!(reader.read_ue())?;
                }
            }
            2 => {}
            _ => track_panic!(ErrorKind::InvalidInput),
        }
        let _num_ref_frames = track!(reader.read_ue())?;
        let _gaps_in_frame_num_value_allowed_flag = track!(reader.read_bit())?;
        let pic_width_in_mbs_minus_1 = track!(reader.read_ue())?;
        let pic_height_in_map_units_minus_1 = track!(reader.read_ue())?;
        let frame_mbs_only_flag = track!(reader.read_bit())?;
        if frame_mbs_only_flag == 0 {
            let _mb_adaptive_frame_field_flag = track!(reader.read_bit())?;
        }
        let _direct_8x8_inference_flag = track!(reader.read_bit())?;
        let frame_cropping_flag = track!(reader.read_bit())?;
        let (
            frame_crop_left_offset,
            frame_crop_right_offset,
            frame_crop_top_offset,
            frame_crop_bottom_offset,
        ) = if frame_cropping_flag == 1 {
            (
                track!(reader.read_ue())?,
                track!(reader.read_ue())?,
                track!(reader.read_ue())?,
                track!(reader.read_ue())?,
            )
        } else {
            (0, 0, 0, 0)
        };

        Ok(SpsSummary {
            profile_idc,
            constraint_set_flag,
            level_idc,
            pic_width_in_mbs_minus_1,
            pic_height_in_map_units_minus_1,
            frame_mbs_only_flag,
            frame_crop_left_offset,
            frame_crop_right_offset,
            frame_crop_top_offset,
            frame_crop_bottom_offset,
            extended_configuration_data: extended_data,
        })
    }
}

#[derive(Debug)]
pub struct NalUnit {
    pub nal_ref_idc: u8,
    pub nal_unit_type: NalUnitType,
}
impl NalUnit {
    pub fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let b = track_io!(reader.read_u8())?;

        let nal_ref_idc = (b >> 5) & 0b11;
        let nal_unit_type = track!(NalUnitType::from_u8(b & 0b1_1111))?;
        Ok(NalUnit {
            nal_ref_idc,
            nal_unit_type,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NalUnitType {
    CodedSliceOfANonIdrPicture = 1,
    CodedSliceDataPartitionA = 2,
    CodedSliceDataPartitionB = 3,
    CodedSliceDataPartitionC = 4,
    CodedSliceOfAnIdrPicture = 5,
    SupplementalEnhancementInformation = 6,
    SequenceParameterSet = 7,
    PictureParameterSet = 8,
    AccessUnitDelimiter = 9,
    EndOfSequence = 10,
    EndOfStream = 11,
    FilterData = 12,
    SequenceParameterSetExtension = 13,
    PrefixNalUnit = 14,
    SubsetSequenceParameterSet = 15,
    CodedSliceOfAnAuxiliaryCodedPictureWithoutPartitioning = 19,
    CodedSliceExtension = 20,
    CodedSliceExtensionForDepthViewComponents = 21,
}
impl NalUnitType {
    fn from_u8(n: u8) -> Result<Self> {
        Ok(match n {
            1 => NalUnitType::CodedSliceOfANonIdrPicture,
            2 => NalUnitType::CodedSliceDataPartitionA,
            3 => NalUnitType::CodedSliceDataPartitionB,
            4 => NalUnitType::CodedSliceDataPartitionC,
            5 => NalUnitType::CodedSliceOfAnIdrPicture,
            6 => NalUnitType::SupplementalEnhancementInformation,
            7 => NalUnitType::SequenceParameterSet,
            8 => NalUnitType::PictureParameterSet,
            9 => NalUnitType::AccessUnitDelimiter,
            10 => NalUnitType::EndOfSequence,
            11 => NalUnitType::EndOfStream,
            12 => NalUnitType::FilterData,
            13 => NalUnitType::SequenceParameterSetExtension,
            14 => NalUnitType::PrefixNalUnit,
            15 => NalUnitType::SubsetSequenceParameterSet,
            19 => NalUnitType::CodedSliceOfAnAuxiliaryCodedPictureWithoutPartitioning,
            20 => NalUnitType::CodedSliceExtension,
            21 => NalUnitType::CodedSliceExtensionForDepthViewComponents,
            _ => track_panic!(ErrorKind::InvalidInput),
        })
    }
}

#[derive(Debug)]
pub struct ByteStreamFormatNalUnits<'a> {
    bytes: &'a [u8],
}
impl<'a> ByteStreamFormatNalUnits<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        let bytes = if bytes.starts_with(&[0, 0, 1][..]) {
            &bytes[3..]
        } else if bytes.starts_with(&[0, 0, 0, 1][..]) {
            &bytes[4..]
        } else {
            track_panic!(ErrorKind::InvalidInput);
        };
        Ok(ByteStreamFormatNalUnits { bytes })
    }
}
impl<'a> Iterator for ByteStreamFormatNalUnits<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            None
        } else {
            let mut nal_unit_end = self.bytes.len();
            let mut next_start = self.bytes.len();
            for i in 0..self.bytes.len() {
                if (&self.bytes[i..]).starts_with(&[0, 0, 0, 1][..]) {
                    nal_unit_end = i;
                    next_start = i + 4;
                    break;
                } else if (&self.bytes[i..]).starts_with(&[0, 0, 1][..]) {
                    nal_unit_end = i;
                    next_start = i + 3;
                    break;
                }
            }
            let nal_unit = &self.bytes[..nal_unit_end];
            self.bytes = &self.bytes[next_start..];
            Some(nal_unit)
        }
    }
}
