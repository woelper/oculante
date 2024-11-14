### Deps:
### cavif-rs      https://github.com/kornelski/cavif-rs
### libavif       https://github.com/AOMediaCodec/libavif
### exiftool      https://github.com/exiftool/exiftool

KARIMI=./mohsen-karimi-f_2B1vBMaQQ-unsplash.jpg

#libavif - aom

avifenc -s 10 -d 8 -y 400 -c aom $KARIMI ./avifenc-depth8_yuv400_aom.avif
avifenc -s 10 -d 8 -y 420 -c aom $KARIMI ./avifenc-depth8_yuv420_aom.avif
avifenc -s 10 -d 8 -y 422 -c aom $KARIMI ./avifenc-depth8_yuv422_aom.avif
avifenc -s 10 -d 8 -y 444 -c aom $KARIMI ./avifenc-depth8_yuv444_aom.avif

avifenc -s 10 -d 8 -y 400 -c aom -r limited $KARIMI ./avifenc-depth8_yuv400_aom_limited.avif
avifenc -s 10 -d 8 -y 420 -c aom -r limited $KARIMI ./avifenc-depth8_yuv420_aom_limited.avif
avifenc -s 10 -d 8 -y 422 -c aom -r limited $KARIMI ./avifenc-depth8_yuv422_aom_limited.avif
avifenc -s 10 -d 8 -y 444 -c aom -r limited $KARIMI ./avifenc-depth8_yuv444_aom_limited.avif

avifenc -s 10 -d 10 -y 400 -c aom $KARIMI ./avifenc-depth10_yuv400_aom.avif
avifenc -s 10 -d 10 -y 420 -c aom $KARIMI ./avifenc-depth10_yuv420_aom.avif
avifenc -s 10 -d 10 -y 422 -c aom $KARIMI ./avifenc-depth10_yuv422_aom.avif
avifenc -s 10 -d 10 -y 444 -c aom $KARIMI ./avifenc-depth10_yuv444_aom.avif

avifenc -s 10 -d 10 -y 400 -c aom -r limited $KARIMI ./avifenc-depth10_yuv400_aom_limited.avif
avifenc -s 10 -d 10 -y 420 -c aom -r limited $KARIMI ./avifenc-depth10_yuv420_aom_limited.avif
avifenc -s 10 -d 10 -y 422 -c aom -r limited $KARIMI ./avifenc-depth10_yuv422_aom_limited.avif
avifenc -s 10 -d 10 -y 444 -c aom -r limited $KARIMI ./avifenc-depth10_yuv444_aom_limited.avif

avifenc -s 10 -d 12 -y 400 -c aom $KARIMI ./avifenc-depth12_yuv400_aom.avif
avifenc -s 10 -d 12 -y 420 -c aom $KARIMI ./avifenc-depth12_yuv420_aom.avif
avifenc -s 10 -d 12 -y 422 -c aom $KARIMI ./avifenc-depth12_yuv422_aom.avif
avifenc -s 10 -d 12 -y 444 -c aom $KARIMI ./avifenc-depth12_yuv444_aom.avif

avifenc -s 10 -d 12 -y 400 -c aom -r limited $KARIMI ./avifenc-depth12_yuv400_aom_limited.avif
avifenc -s 10 -d 12 -y 420 -c aom -r limited $KARIMI ./avifenc-depth12_yuv420_aom_limited.avif
avifenc -s 10 -d 12 -y 422 -c aom -r limited $KARIMI ./avifenc-depth12_yuv422_aom_limited.avif
avifenc -s 10 -d 12 -y 444 -c aom -r limited $KARIMI ./avifenc-depth12_yuv444_aom_limited.avif

#libavif - rav1e

avifenc -s 10 -d 8 -y 400 -c rav1e $KARIMI ./avifenc-depth8_yuv400_rav1e.avif
avifenc -s 10 -d 8 -y 420 -c rav1e $KARIMI ./avifenc-depth8_yuv420_rav1e.avif
avifenc -s 10 -d 8 -y 422 -c rav1e $KARIMI ./avifenc-depth8_yuv422_rav1e.avif
avifenc -s 10 -d 8 -y 444 -c rav1e $KARIMI ./avifenc-depth8_yuv444_rav1e.avif

avifenc -s 10 -d 8 -y 400 -c rav1e -r limited $KARIMI ./avifenc-depth8_yuv400_rav1e_limited.avif
avifenc -s 10 -d 8 -y 420 -c rav1e -r limited $KARIMI ./avifenc-depth8_yuv420_rav1e_limited.avif
avifenc -s 10 -d 8 -y 422 -c rav1e -r limited $KARIMI ./avifenc-depth8_yuv422_rav1e_limited.avif
avifenc -s 10 -d 8 -y 444 -c rav1e -r limited $KARIMI ./avifenc-depth8_yuv444_rav1e_limited.avif

avifenc -s 10 -d 10 -y 400 -c rav1e $KARIMI ./avifenc-depth10_yuv400_rav1e.avif
avifenc -s 10 -d 10 -y 420 -c rav1e $KARIMI ./avifenc-depth10_yuv420_rav1e.avif
avifenc -s 10 -d 10 -y 422 -c rav1e $KARIMI ./avifenc-depth10_yuv422_rav1e.avif
avifenc -s 10 -d 10 -y 444 -c rav1e $KARIMI ./avifenc-depth10_yuv444_rav1e.avif

avifenc -s 10 -d 10 -y 400 -c rav1e -r limited $KARIMI ./avifenc-depth10_yuv400_rav1e_limited.avif
avifenc -s 10 -d 10 -y 420 -c rav1e -r limited $KARIMI ./avifenc-depth10_yuv420_rav1e_limited.avif
avifenc -s 10 -d 10 -y 422 -c rav1e -r limited $KARIMI ./avifenc-depth10_yuv422_rav1e_limited.avif
avifenc -s 10 -d 10 -y 444 -c rav1e -r limited $KARIMI ./avifenc-depth10_yuv444_rav1e_limited.avif

avifenc -s 10 -d 10 -y 400 -c rav1e $KARIMI ./avifenc-depth12_yuv400_rav1e.avif
avifenc -s 10 -d 10 -y 420 -c rav1e $KARIMI ./avifenc-depth12_yuv420_rav1e.avif
avifenc -s 10 -d 10 -y 422 -c rav1e $KARIMI ./avifenc-depth12_yuv422_rav1e.avif
avifenc -s 10 -d 10 -y 444 -c rav1e $KARIMI ./avifenc-depth12_yuv444_rav1e.avif

avifenc -s 10 -d 10 -y 400 -c rav1e -r limited $KARIMI ./avifenc-depth12_yuv400_rav1e_limited.avif
avifenc -s 10 -d 10 -y 420 -c rav1e -r limited $KARIMI ./avifenc-depth12_yuv420_rav1e_limited.avif
avifenc -s 10 -d 10 -y 422 -c rav1e -r limited $KARIMI ./avifenc-depth12_yuv422_rav1e_limited.avif
avifenc -s 10 -d 10 -y 444 -c rav1e -r limited $KARIMI ./avifenc-depth12_yuv444_rav1e_limited.avif

#libavif - svt

avifenc -s 10 -d 8 -y 420 -c svt $KARIMI ./avifenc-depth8_yuv420_svt.avif

avifenc -s 10 -d 8 -y 420 -c svt -r limited $KARIMI ./avifenc-depth8_yuv420_svt_limited.avif

avifenc -s 10 -d 10 -y 420 -c svt $KARIMI ./avifenc-depth10_yuv420_svt.avif

avifenc -s 10 -d 10 -y 420 -c svt -r limited $KARIMI ./avifenc-depth10_yuv420_svt_limited.avif

#cavif-rs

cavif -s 10 -Q 80 --depth 8 --color ycbcr $KARIMI -o ./cafiv-depth8_ycbcr.avif

cavif -s 10 -Q 80 --depth 8 --color rgb $KARIMI -o ./cafiv-depth8_rgb.avif

cavif -s 10 -Q 80 --depth 10 --color ycbcr $KARIMI -o ./cafiv-depth10_ycbcr.avif

cavif -s 10 -Q 80 --depth 10 --color rgb $KARIMI -o ./cafiv-depth10_rgb.avif

# imagemagick formats

convert moss.jpg ./format-test.bmp
convert moss.jpg ./format-test.jpeg
convert moss.jpg ./format-test.png
convert moss.jpg ./format-test.jxl
convert moss.jpg ./format-test.avif
convert moss.jpg ./format-test.tiff
convert moss.jpg ./format-test.webp
convert moss.jpg ./format-test.farbfeld
convert moss.jpg ./format-test.dds
convert moss.jpg ./format-test.ppm
convert moss.jpg ./format-test.heic
convert moss.jpg ./format-test.qoi

# imagemagick odd width

convert -resize 4601x6900! moss.jpg ./odd_width-format-test.bmp
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.jpeg
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.png
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.jxl
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.avif
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.tiff
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.webp
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.farbfeld
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.dds
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.ppm
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.heic
convert -resize 4601x6900! moss.jpg ./odd_width-format-test.qoi

# imagemagick odd height

convert -resize 4600x6901! moss.jpg ./odd_height-format-test.bmp
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.jpeg
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.png
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.jxl
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.avif
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.tiff
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.webp
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.farbfeld
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.dds
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.ppm
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.heic
convert -resize 4600x6901! moss.jpg ./odd_height-format-test.qoi

# imagemagick odd width and height

convert -resize 4601x6901! moss.jpg ./odd-format-test.bmp
convert -resize 4601x6901! moss.jpg ./odd-format-test.jpeg
convert -resize 4601x6901! moss.jpg ./odd-format-test.png
convert -resize 4601x6901! moss.jpg ./odd-format-test.jxl
convert -resize 4601x6901! moss.jpg ./odd-format-test.avif
convert -resize 4601x6901! moss.jpg ./odd-format-test.tiff
convert -resize 4601x6901! moss.jpg ./odd-format-test.webp
convert -resize 4601x6901! moss.jpg ./odd-format-test.farbfeld
convert -resize 4601x6901! moss.jpg ./odd-format-test.dds
convert -resize 4601x6901! moss.jpg ./odd-format-test.ppm
convert -resize 4601x6901! moss.jpg ./odd-format-test.heic
convert -resize 4601x6901! moss.jpg ./odd-format-test.qoi

# uppercase drag and drop extension

cp ./drag_and_drop.png ./drag_and_drop.PNG
cp ./drag_and_drop.png ./drag_and_drop.PnG

# imagemagick odd drag and drop

convert -resize 799x800! ./drag_and_drop.png ./drag_and_drop_oddw.png
convert -resize 800x799! ./drag_and_drop.png ./drag_and_drop_oddh.png
convert -resize 799x799! ./drag_and_drop.png ./drag_and_drop_oddwh.png

# imagemagick jxl up to 1 gigapixel

convert -monitor -resize 10000x10000 ./gradient_mesh.jxl gradient_mesh_10k.jxl
convert -monitor -resize 20000x20000 ./gradient_mesh.jxl gradient_mesh_20k.jxl
convert -monitor -resize 31623x31623 ./gradient_mesh.jxl gradient_mesh_31623-gigapixel.jxl

# imagemagick moss formats

convert moss.jpg exiftool-left_moss.png
convert moss.jpg exiftool-left_moss.webp

# exiftool rotate prep

cp exiftool-left_moss.png exiftool-right_moss.png

cp exiftool-left_moss.webp exiftool-right_moss.webp

cp moss.jpg exiftool-left_moss.jpg
cp moss.jpg exiftool-right_moss.jpg

# exiftool rotate

exiftool -overwrite_original -Orientation='Rotate 270 CW' exiftool-left_moss.jpg
exiftool -overwrite_original -Orientation='Rotate 270 CW' exiftool-left_moss.png
exiftool -overwrite_original -Orientation='Rotate 270 CW' exiftool-left_moss.webp

exiftool -overwrite_original -Orientation='Rotate 90 CW' exiftool-right_moss.jpg
exiftool -overwrite_original -Orientation='Rotate 90 CW' exiftool-right_moss.png
exiftool -overwrite_original -Orientation='Rotate 90 CW' exiftool-right_moss.webp

# incorrect format

cp rust.png png_rust.jpg
cp orange.heic heic_orange.png
cp pngtest_16bit.png png_pngtest_16bit.webp
