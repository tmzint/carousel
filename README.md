# Carousel

## WGSL Shader validation

```bash
git clone https://github.com/gfx-rs/naga.git
cd naga
cargo run --features wgsl-in -- *.wgsl
```

## Fonts

check out `https://github.com/Chlumsky/msdf-atlas-gen`

install dependencies:
```
sudo dnf install freetype freetype-devel
```

Generate atlas:

```
./msdf-atlas-gen -font <fontfile.ttf/otf> -json <fontfile.json> -imageout <fontfile.png> [-type <msdf/softmask/hardmask>] [-charset <charset.txt>] [-pxrange <2..>] [-size <32>] 
```

Example:
```
./msdf-atlas-gen -font Hack-v3.003-ttf/ttf/Hack-Regular.ttf -json hack_regular.json -imageout hack_regular.png -size 24 -pxrange 12
```

## Licence

Roundabout is dual-licensed under Apache 2.0 and MIT.

See [LICENSE_APACHE](LICENSE_APACHE) and [LICENSE_MIT](LICENSE_MIT)