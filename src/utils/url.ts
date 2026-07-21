export function normalizePublicUrl(rawValue: string) {
  const parsed = new URL(rawValue.trim());
  if (!["http:", "https:"].includes(parsed.protocol)) {
    throw new Error("仅支持 HTTP 或 HTTPS URL");
  }
  if (parsed.username || parsed.password) {
    throw new Error("URL 不能包含账号或密码");
  }
  parsed.hash = "";
  return parsed.toString();
}
