These DICOM files are from the tests of pydicom,
https://github.com/pydicom/pydicom/tree/main/src/pydicom/data/test_files

- CT_small.dcm: CT, 16 bit, uncompressed (Explicit VR Little Endian)
- MR_small_RLE.dcm: MR, 16 bit, RLE Lossless
- JPGExtended.dcm: NM, 12 bit, JPEG Extended
- MR_small_jp2klossless.dcm: MR, 16 bit, JPEG 2000 Lossless
- MR_small_jpeg_ls_lossless.dcm: MR, 16 bit, JPEG-LS Lossless

The images were downsized and anonymised by pydicom from the NEMA WG04 test
sets, ftp://medical.nema.org/MEDICAL/Dicom/DataSets/WG04/

SPDX-FileCopyrightText: 2008-2020 Darcy Mason and pydicom contributors
SPDX-License-Identifier: MIT, see LICENSE-pydicom.txt
