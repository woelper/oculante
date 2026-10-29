# Downloads camera RAW files from raw.pixls.us (CC0) into target/raw-samples, for
# the test raw_samples. They are too big for the repository, about 430 MB.
#   scripts/raw-samples.sh && cargo test raw_samples -- --ignored
set -e

cd "$(dirname "$0")/.."
mkdir -p target/raw-samples
cd target/raw-samples

while read -r sha1 path; do
    file=$(basename "$path")
    if ! echo "$sha1  $file" | sha1sum -c --status 2>/dev/null; then
        echo "Downloading $file"
        curl -fsSL -o "$file" "https://raw.pixls.us/data/$path"
        echo "$sha1  $file" | sha1sum -c --quiet
    fi
done <<'LIST'
3333e89a668971ba0354e854750376330d145749 Nikon/D850/Nikon-D850-14bit-compressed.NEF
a616f265278124a13658ac535c6eafda187a87cb Nikon/Z%206/DSC_0750.NEF
79013086cbb129b8adbe087496d5cca1edf61244 Canon/EOS%205D%20Mark%20IV/B13A0729.CR2
1065866a5cd1c88f73f36b534508af4aff534b5e Canon/EOS%20R5/Canon_EOS_R5_CRAW_ISO_100_nocrop_nodual.CR3
0ef233dc974d8dfd64c9f1a04a252e952205a5cf Canon/EOS%20R6/Canon_EOS_R6_RAW_ISO_100_nocrop_nodual.CR3
46ef96914aab9b3c1e56d5544b29c52b7052cf57 Sony/ILCE-7M3/_DSC0009.ARW
701d08de6b485f31e4bda75f2481d0c192623c23 Fujifilm/X-T3/AFXT2720.RAF
a56da2c412e85b65138dbab0636b31dd082e1ecf Panasonic/DC-GH5/_T012010.RW2
69aed8001dc2fd804ce3ddd5bc50e2fc59d3f925 Olympus/E-M1MarkII/Olympus_EM1mk2_Standard_20MP.ORF
e557fface1cd693ded7d787019449c0396a608c3 Pentax/K-1/IMGP8550.PEF
2e15e1d9f7576226d17341777010ece5797e841a Ricoh/GR%20III/R0000357.DNG
c05401ea559cffb1b5c1332f4689cec241830d2b Apple/iPhone%2012%20Pro/IMG_1361.DNG
LIST
