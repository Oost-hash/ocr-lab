export interface ImageInput {
  filename: string;
  sha256: string;
  widthPx: number;
  heightPx: number;
  path: string;
}

export interface ImageState {
  path: string | null;
  previewUrl: string | null;
  input: ImageInput | null;
}
