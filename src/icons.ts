import { addIcon } from "@iconify/vue";
import alignLeft from "@iconify-icons/lucide/align-left";
import arrowLeft from "@iconify-icons/lucide/arrow-left";
import check from "@iconify-icons/lucide/check";
import circleSlash from "@iconify-icons/lucide/circle-slash-2";
import clock from "@iconify-icons/lucide/clock-3";
import chevronDown from "@iconify-icons/lucide/chevron-down";
import chevronLeft from "@iconify-icons/lucide/chevron-left";
import chevronRight from "@iconify-icons/lucide/chevron-right";
import codeXml from "@iconify-icons/lucide/code-2";
import copy from "@iconify-icons/lucide/copy";
import eye from "@iconify-icons/lucide/eye";
import externalLink from "@iconify-icons/lucide/external-link";
import fileCheck from "@iconify-icons/lucide/file-check";
import fileSearch from "@iconify-icons/lucide/file-search";
import fileWarning from "@iconify-icons/lucide/file-warning";
import folderOpen from "@iconify-icons/lucide/folder-open";
import imageOff from "@iconify-icons/lucide/image-off";
import info from "@iconify-icons/lucide/info";
import listTree from "@iconify-icons/lucide/list-tree";
import library from "@iconify-icons/lucide/library";
import loaderCircle from "@iconify-icons/lucide/loader-2";
import maximize from "@iconify-icons/lucide/maximize";
import minus from "@iconify-icons/lucide/minus";
import rotateCw from "@iconify-icons/lucide/rotate-cw";
import save from "@iconify-icons/lucide/save";
import scanEye from "@iconify-icons/lucide/scan-eye";
import search from "@iconify-icons/lucide/search";
import settings from "@iconify-icons/lucide/settings";
import star from "@iconify-icons/lucide/star";
import starOff from "@iconify-icons/lucide/star-off";
import square from "@iconify-icons/lucide/square";
import shieldCheck from "@iconify-icons/lucide/shield-check";
import triangleAlert from "@iconify-icons/lucide/alert-triangle";
import x from "@iconify-icons/lucide/x";
import zoomIn from "@iconify-icons/lucide/zoom-in";
import zoomOut from "@iconify-icons/lucide/zoom-out";

const icons = {
  "align-left": alignLeft,
  "arrow-left": arrowLeft,
  check,
  "circle-slash-2": circleSlash,
  "clock-3": clock,
  "chevron-down": chevronDown,
  "chevron-left": chevronLeft,
  "chevron-right": chevronRight,
  "code-xml": codeXml,
  copy,
  eye,
  "external-link": externalLink,
  "file-check": fileCheck,
  "file-search": fileSearch,
  "file-warning": fileWarning,
  "folder-open": folderOpen,
  "image-off": imageOff,
  info,
  "list-tree": listTree,
  library,
  "loader-circle": loaderCircle,
  maximize,
  minus,
  "rotate-cw": rotateCw,
  save,
  "scan-eye": scanEye,
  search,
  settings,
  star,
  "star-off": starOff,
  square,
  "shield-check": shieldCheck,
  "triangle-alert": triangleAlert,
  x,
  "zoom-in": zoomIn,
  "zoom-out": zoomOut,
} as const;

// Material Design Icons 的官方实心星标，用于区分已收藏状态。
const filledStar = {
  width: 24,
  height: 24,
  body: '<path fill="currentColor" d="M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.62L12 2L9.19 8.62L2 9.24l5.45 4.73L5.82 21L12 17.27Z"/>',
};

export function installIcons() {
  for (const [name, icon] of Object.entries(icons)) addIcon(`lucide:${name}`, icon);
  addIcon("mdi:star", filledStar);
}
