export interface ChapterItem {
  id: string;
  label: string;
  children?: ChapterItem[];
}
