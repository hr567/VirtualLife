//! PCX support for three original end-fight pictures mislabeled as .BMP.
use crate::{binary::{Reader,pixel_count},bmp::Image};

pub fn decode(data:&[u8])->Result<Image,String> {
    if data.len()<128 {return Err("Truncated PCX header".into());}
    let mut r=Reader::new(data);
    if r.u8()?!=0x0a {return Err("Not a PCX image".into());}
    r.skip(1)?;
    let encoding=r.u8()?; let depth=r.u8()?;
    if encoding>1 || depth!=8 {return Err("Unsupported PCX encoding or bit depth".into());}
    let left=r.u16()? as usize; let top=r.u16()? as usize;
    let right=r.u16()? as usize; let bottom=r.u16()? as usize;
    if right<left || bottom<top {return Err("Invalid PCX bounds".into());}
    let width=right-left+1; let height=bottom-top+1;
    let count=pixel_count(width,height)?;
    let planes=data[65] as usize;
    let stride=u16::from_le_bytes([data[66],data[67]]) as usize;
    if ![1,3].contains(&planes) || stride<width || stride>65535 {return Err("Unsupported PCX plane layout".into());}
    let palette=if planes==1 {
        if data.len()<897 || data[data.len()-769]!=12 {return Err("PCX 256-color palette is missing".into());}
        Some(&data[data.len()-768..])
    }else{None};
    let end=if palette.is_some(){data.len()-769}else{data.len()};
    let mut r=Reader::new(&data[128..end]);
    let mut rgba=vec![0;count*4];
    let mut indices=palette.map(|_|vec![0u8;count]);
    let mut row=vec![0;stride*planes];
    for y in 0..height {
        let mut cursor=0;
        while cursor<row.len() {
            let value=r.u8()?;
            let (length,value)=if encoding==1 && value&0xc0==0xc0 {(usize::from(value&0x3f),r.u8()?)}else{(1,value)};
            if length==0 || cursor+length>row.len(){return Err("PCX run exceeds scan line".into());}
            row[cursor..cursor+length].fill(value); cursor+=length;
        }
        for x in 0..width {
            if let Some(indices)=&mut indices {indices[y*width+x]=row[x];}
            let output=&mut rgba[(y*width+x)*4..][..4];
            if let Some(palette)=palette {output[..3].copy_from_slice(&palette[usize::from(row[x])*3..][..3]);}
            else {output[..3].copy_from_slice(&[row[x],row[stride+x],row[stride*2+x]]);}
            output[3]=255;
        }
    }
    Ok(Image{width:width as u32,height:height as u32,rgba,indices})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexed_pcx_uses_footer_palette() {
        let mut bytes=vec![0u8;128]; bytes[0]=10;bytes[2]=1;bytes[3]=8;bytes[65]=1;bytes[66]=2;
        bytes.extend_from_slice(&[1,0,12]); let mut palette=vec![0;768];palette[3..6].copy_from_slice(&[12,34,56]);bytes.extend(palette);
        assert_eq!(decode(&bytes).unwrap().rgba,[12,34,56,255]);
        assert_eq!(decode(&bytes).unwrap().indices,Some(vec![1]));
    }
}
