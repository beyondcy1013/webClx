package android.view;

public class View {
    private int left;
    private int top;
    private int right;
    private int bottom;
    public int paddingWrites;
    public int insetRequests;

    public int getPaddingLeft() { return left; }
    public int getPaddingTop() { return top; }
    public int getPaddingRight() { return right; }
    public int getPaddingBottom() { return bottom; }

    public void setPadding(int left, int top, int right, int bottom) {
        this.left = left;
        this.top = top;
        this.right = right;
        this.bottom = bottom;
        paddingWrites++;
    }
}
