package dev.acportal.data;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileOutputStream;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;

/** Deterministic fixture bytes in the test APK, with no dependency on target-app classes. */
public final class AttachmentTestProvider extends ContentProvider {
    public boolean onCreate() { return true; }
    public String getType(Uri uri) {
        switch (uri.getLastPathSegment()) {
            case "image": return "image/png";
            case "audio": return "audio/wav";
            case "text": return "text/plain";
            default: return "application/octet-stream";
        }
    }
    public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
        String name;
        switch (uri.getLastPathSegment()) {
            case "text": name="fixture.kt"; break;
            case "image": name="fixture.png"; break;
            case "audio": name="fixture.wav"; break;
            default: name="fixture.bin";
        }
        MatrixCursor cursor=new MatrixCursor(new String[]{OpenableColumns.DISPLAY_NAME});
        cursor.addRow(new Object[]{name});
        return cursor;
    }
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) throw new FileNotFoundException("Read-only fixtures");
        byte[] bytes;
        switch (uri.getLastPathSegment()) {
            case "text": bytes="val greeting = \"Hello, 世界\"".getBytes(StandardCharsets.UTF_8); break;
            case "image":
                android.graphics.Bitmap bitmap=android.graphics.Bitmap.createBitmap(320,180,android.graphics.Bitmap.Config.ARGB_8888);
                bitmap.eraseColor(android.graphics.Color.rgb(22,22,22));
                android.graphics.Canvas canvas=new android.graphics.Canvas(bitmap);
                android.graphics.Paint paint=new android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG);
                paint.setColor(android.graphics.Color.rgb(114,213,176));paint.setTextSize(24);
                canvas.drawText("ACP media fixture",40,96,paint);
                ByteArrayOutputStream image=new ByteArrayOutputStream();bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,image);bitmap.recycle();bytes=image.toByteArray();break;
            case "audio": bytes=ByteBuffer.allocate(46).order(ByteOrder.LITTLE_ENDIAN)
                .put("RIFF".getBytes(StandardCharsets.US_ASCII)).putInt(38)
                .put("WAVEfmt ".getBytes(StandardCharsets.US_ASCII)).putInt(16)
                .putShort((short)1).putShort((short)1).putInt(16000).putInt(32000)
                .putShort((short)2).putShort((short)16)
                .put("data".getBytes(StandardCharsets.US_ASCII)).putInt(2).putShort((short)0).array(); break;
            case "large": bytes=new byte[262145]; Arrays.fill(bytes,(byte)'a'); break;
            default: bytes=new byte[]{0,(byte)255,1,2};
        }
        File file=new File(getContext().getCacheDir(),"attachment-"+uri.getLastPathSegment()+".fixture");
        try (FileOutputStream output=new FileOutputStream(file)) { output.write(bytes); }
        catch (IOException error) { throw new FileNotFoundException("Fixture unavailable"); }
        return ParcelFileDescriptor.open(file,ParcelFileDescriptor.MODE_READ_ONLY);
    }
    public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException(); }
    public int delete(Uri uri, String selection, String[] args) { throw new UnsupportedOperationException(); }
    public int update(Uri uri, ContentValues values, String selection, String[] args) { throw new UnsupportedOperationException(); }
}
