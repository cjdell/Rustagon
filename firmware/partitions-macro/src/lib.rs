use esp_idf_part::PartitionTable;
use image::GenericImageView as _;
use litrs::Literal;
use proc_macro::TokenStream;
use proc_macro_error::{abort_call_site, proc_macro_error};
use quote::quote;
use syn::{parse_macro_input, Expr};

#[proc_macro_error]
#[proc_macro]
pub fn partition_offset(input: TokenStream) -> TokenStream {
  let first_token = input
    .into_iter()
    .next()
    .unwrap_or_else(|| abort_call_site!("Expected a partition name"));

  let partname = match Literal::try_from(first_token) {
    Ok(Literal::String(name)) => name.value().to_owned(),
    _ => abort_call_site!("Expected a partition name as string"),
  };

  let csv = std::fs::read_to_string("partitions.csv").unwrap();
  let table = PartitionTable::try_from_str(csv).unwrap();

  let part = table.find(&partname).expect("No partition found");
  let offset = part.offset();

  quote! {
      #offset
  }
  .into()
}

#[proc_macro_error]
#[proc_macro]
pub fn partition_size(input: TokenStream) -> TokenStream {
  let first_token = input
    .into_iter()
    .next()
    .unwrap_or_else(|| abort_call_site!("Expected a partition name"));

  let partname = match Literal::try_from(first_token) {
    Ok(Literal::String(name)) => name.value().to_owned(),
    _ => abort_call_site!("Expected a partition name as string"),
  };

  let csv = std::fs::read_to_string("partitions.csv").unwrap();
  let table = PartitionTable::try_from_str(csv).unwrap();

  let part = table.find(&partname).expect("No partition found");
  let size = part.size();

  quote! {
      #size
  }
  .into()
}

// Convert RGBA8888 to RGB565 with black background for transparent pixels
fn rgba_to_rgb565(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
  let mut result = Vec::with_capacity((width * height) as usize * 2);
  let stride = width as usize * 4; // 4 bytes per pixel (RGBA)

  for y in 0..height {
    for x in 0..width {
      let idx = (y as usize * stride) + (x as usize * 4);
      let r = pixels[idx];
      let g = pixels[idx + 1];
      let b = pixels[idx + 2];
      let a = pixels[idx + 3];

      // If transparent, use black (0,0,0), else use original color
      // let (r, g, b) = if a == 0 { (0, 0, 0) } else { (r, g, b) };
      let (r, g, b) = (
        ((r as u32 * a as u32) / 255) as u8,
        ((g as u32 * a as u32) / 255) as u8,
        ((b as u32 * a as u32) / 255) as u8,
      );

      // Convert to RGB565: R(5) G(6) B(5)
      let r5: u16 = (r as u16 >> 3) & 0x1F; // 5 bits
      let g6: u16 = (g as u16 >> 2) & 0x3F; // 6 bits
      let b5: u16 = (b as u16 >> 3) & 0x1F; // 5 bits

      let pixel16 = (r5 << 11) | (g6 << 5) | b5;
      let hi = (pixel16 >> 8) as u8;
      let lo = pixel16 as u8;
      result.push(hi);
      result.push(lo);
    }
  }

  result
}

#[proc_macro]
pub fn include_rgb565_icon(input: TokenStream) -> TokenStream {
  let expr = parse_macro_input!(input as Expr);

  // Extract the string literal path
  let path = match &expr {
    Expr::Lit(lit) => match &lit.lit {
      syn::Lit::Str(s) => s.value(),
      _ => panic!("Expected a string literal, e.g., \"icons/icon.png\""),
    },
    _ => panic!("Expected a string literal, e.g., \"icons/icon.png\""),
  };

  // Read the image file
  let img = image::open(&path).unwrap_or_else(|_| panic!("Failed to open image file: {}", path));

  // Ensure it's 20x20
  let (width, height) = img.dimensions();
  // if width != 20 || height != 20 {
  //   panic!("Icon must be exactly 20x20 pixels, got {}x{}", width, height);
  // }

  // Convert to RGBA
  let rgba = img.to_rgba8();
  let pixels = rgba.as_raw();

  // Convert to RGB565
  let rgb565_data = rgba_to_rgb565(pixels, width, height);

  // Ensure we have exactly 800 bytes (20*20*2)
  if rgb565_data.len() != (width * height * 2) as usize {
    panic!(
      "RGB565 data must be {} bytes, got {}",
      width * height * 2,
      rgb565_data.len()
    );
  }

  // Generate a static array of u8 with the data
  let byte_array = rgb565_data.iter().map(|&b| quote! { #b }).collect::<Vec<_>>();

  let output = quote! {
    &[#(#byte_array),*]
  };

  // let output = quote! {
  //     {
  //         #[allow(non_snake_case)]
  //         const ICON_DATA: &[u8] = &[#(#byte_array),*];
  //         ICON_DATA
  //     }
  // };

  TokenStream::from(output)
}
