package android.content.pm;

public class PackageManager {
    private PackageInfo info = new PackageInfo();

    public PackageInfo getPackageInfo(String packageName, int flags) {
        return info;
    }

    public void setInstalledVersionCode(int code) {
        info.versionCode = code;
    }
}
