package android.content.pm;

public class PackageInfo {
    public String packageName = "com.webclx.app";
    public String versionName = "1.8.21";
    public int versionCode = 1008021;
    public Object signatures;
    public Object signingInfo;

    public long getLongVersionCode() {
        return versionCode;
    }
}
