package com.hse.bleradar;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RadialGradient;
import android.graphics.RectF;
import android.graphics.Shader;
import android.graphics.SweepGradient;
import android.os.SystemClock;
import android.util.AttributeSet;
import android.view.View;

import java.util.ArrayList;
import java.util.List;

/**
 * Circular sweep radar display. Visually analogous to the retained oracle
 * APK's own launcher icon (dark background, mint-green concentric rings
 * that dim toward the centre, and colour-coded device blips) but drawn full
 * size on a live {@link Canvas} rather than as a static icon: range rings
 * with distance labels, a continuously rotating sweep wedge, and a blip per
 * tracked {@link Blip}, colour-coded by {@code NativeRadar.PROXIMITY_*}.
 *
 * <p>Ownership: {@link MainActivity} pushes the latest snapshot of tracked
 * devices via {@link #setBlips(List)}; this view owns only the sweep-angle
 * animation clock and pure rendering, so it never blocks on BLE I/O.
 */
public final class RadarView extends View {

    private static final float SWEEP_ARC_DEGREES = 50f;
    private static final long SWEEP_PERIOD_MILLIS = 4200L;
    private static final long BLIP_FRESHNESS_WINDOW_MILLIS = 9000L;
    private static final int RING_COUNT = 4;

    private double maxRangeMetres = 40.0;
    private List<Blip> blips = new ArrayList<>();
    private final long animationStartUptimeMillis = SystemClock.uptimeMillis();

    private final Paint backgroundPaint = solidPaint(Color.parseColor("#0F1419"));
    private final Paint ringPaint = strokePaint(Color.parseColor("#39D98A"));
    private final Paint centrePaint = solidPaint(Color.parseColor("#39D98A"));
    private final Paint ringLabelPaint = textPaint(Color.parseColor("#7E9C90"), 11f);
    private final Paint blipLabelPaint = textPaint(Color.parseColor("#E7F3ED"), 12f);
    private final Paint blipLabelBackgroundPaint = solidPaint(Color.parseColor("#CC162026"));
    private final Paint sweepPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF sweepBounds = new RectF();

    public RadarView(Context context) {
        super(context);
    }

    public RadarView(Context context, AttributeSet attrs) {
        super(context, attrs);
    }

    /** Maximum distance, in metres, mapped to the outermost ring. Must be positive and finite. */
    public void setMaxRangeMetres(double maxRangeMetres) {
        if (maxRangeMetres > 0 && Double.isFinite(maxRangeMetres)) {
            this.maxRangeMetres = maxRangeMetres;
        }
    }

    /** Replaces the rendered device snapshot. Safe to call from the main thread only. */
    public void setBlips(List<Blip> blips) {
        this.blips = blips;
        invalidate();
    }

    private static Paint solidPaint(int color) {
        Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        paint.setColor(color);
        paint.setStyle(Paint.Style.FILL);
        return paint;
    }

    private static Paint strokePaint(int color) {
        Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        paint.setColor(color);
        paint.setStyle(Paint.Style.STROKE);
        return paint;
    }

    private static Paint textPaint(int color, float spSize) {
        Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        paint.setColor(color);
        paint.setTextSize(spSize * 3f);
        paint.setTextAlign(Paint.Align.CENTER);
        return paint;
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        int width = getWidth();
        int height = getHeight();
        float cx = width / 2f;
        float cy = height / 2f;
        float radius = Math.min(width, height) / 2f * 0.9f;

        canvas.drawRect(0, 0, width, height, backgroundPaint);
        if (radius <= 0) {
            return;
        }

        drawRings(canvas, cx, cy, radius);
        drawSweep(canvas, cx, cy, radius);
        drawBlips(canvas, cx, cy, radius);

        // Centre marker: the observing device itself.
        canvas.drawCircle(cx, cy, Math.max(4f, radius * 0.02f), centrePaint);

        postInvalidateOnAnimation();
    }

    private void drawRings(Canvas canvas, float cx, float cy, float radius) {
        for (int i = 1; i <= RING_COUNT; i++) {
            float fraction = i / (float) RING_COUNT;
            // Fainter near the centre, brightest at the outer edge — matches
            // the extracted original icon's own alpha progression exactly.
            int alpha = Math.round(90 + 165 * fraction);
            ringPaint.setAlpha(Math.min(255, alpha));
            ringPaint.setStrokeWidth(Math.max(1.5f, radius * 0.012f));
            canvas.drawCircle(cx, cy, radius * fraction, ringPaint);

            double labelMetres = maxRangeMetres * fraction;
            canvas.drawText(
                    formatMetres(labelMetres),
                    cx,
                    cy - radius * fraction + ringLabelPaint.getTextSize(),
                    ringLabelPaint);
        }
    }

    private void drawSweep(Canvas canvas, float cx, float cy, float radius) {
        long elapsed = SystemClock.uptimeMillis() - animationStartUptimeMillis;
        float sweepAngle = (elapsed % SWEEP_PERIOD_MILLIS) / (float) SWEEP_PERIOD_MILLIS * 360f;

        sweepBounds.set(cx - radius, cy - radius, cx + radius, cy + radius);
        int trailing = Color.argb(0, 0x39, 0xD9, 0x8A);
        int leading = Color.argb(140, 0x39, 0xD9, 0x8A);
        SweepGradient shader = new SweepGradient(
                cx, cy,
                new int[] {trailing, trailing, leading},
                new float[] {0f, 1f - (SWEEP_ARC_DEGREES / 360f), 1f});
        sweepPaint.setShader(shader);
        sweepPaint.setStyle(Paint.Style.FILL);
        canvas.save();
        canvas.rotate(sweepAngle - SWEEP_ARC_DEGREES, cx, cy);
        canvas.drawArc(sweepBounds, 0f, 360f, true, sweepPaint);
        canvas.restore();
    }

    private void drawBlips(Canvas canvas, float cx, float cy, float radius) {
        long now = SystemClock.uptimeMillis();
        for (Blip blip : blips) {
            if (!blip.isFresh(now, BLIP_FRESHNESS_WINDOW_MILLIS)) {
                continue;
            }
            double distance = Double.isNaN(blip.distanceMetres)
                    ? fallbackDistanceForProximity(blip.proximity)
                    : blip.distanceMetres;
            float normalised = (float) Math.min(1.0, distance / maxRangeMetres);
            double angleRad = Math.toRadians(blip.angleDegrees - 90.0);
            float bx = cx + radius * normalised * (float) Math.cos(angleRad);
            float by = cy + radius * normalised * (float) Math.sin(angleRad);

            long age = now - blip.lastSeenUptimeMillis;
            float freshness = 1f - Math.min(1f, age / (float) BLIP_FRESHNESS_WINDOW_MILLIS);
            int blipColor = colourForProximity(blip.proximity);
            int glowAlpha = Math.round(90 * freshness);
            int coreAlpha = Math.round(255 * (0.55f + 0.45f * freshness));

            float blipRadius = Math.max(5f, radius * 0.035f);
            Paint glow = new Paint(Paint.ANTI_ALIAS_FLAG);
            glow.setShader(new RadialGradient(
                    bx, by, blipRadius * 3.2f,
                    withAlpha(blipColor, glowAlpha), withAlpha(blipColor, 0),
                    Shader.TileMode.CLAMP));
            canvas.drawCircle(bx, by, blipRadius * 3.2f, glow);

            Paint core = new Paint(Paint.ANTI_ALIAS_FLAG);
            core.setColor(withAlpha(blipColor, coreAlpha));
            canvas.drawCircle(bx, by, blipRadius, core);

            String label = shortLabel(blip) + " · " + formatMetres(distance);
            float labelWidth = blipLabelPaint.measureText(label) + 12f;
            float labelTop = by + blipRadius + 2f;
            canvas.drawRoundRect(
                    bx - labelWidth / 2f, labelTop,
                    bx + labelWidth / 2f, labelTop + blipLabelPaint.getTextSize() + 8f,
                    6f, 6f, blipLabelBackgroundPaint);
            canvas.drawText(label, bx, labelTop + blipLabelPaint.getTextSize() + 2f, blipLabelPaint);
        }
    }

    private static String shortLabel(Blip blip) {
        if (blip.name != null && !blip.name.isEmpty()) {
            return blip.name;
        }
        String address = blip.address;
        return address.length() > 5 ? address.substring(address.length() - 5) : address;
    }

    private static double fallbackDistanceForProximity(int proximity) {
        switch (proximity) {
            case NativeRadar.PROXIMITY_IMMEDIATE:
                return 2.0;
            case NativeRadar.PROXIMITY_NEAR:
                return 8.0;
            case NativeRadar.PROXIMITY_MID:
                return 18.0;
            default:
                return 32.0;
        }
    }

    private static int colourForProximity(int proximity) {
        switch (proximity) {
            case NativeRadar.PROXIMITY_IMMEDIATE:
                return Color.parseColor("#FF7A45");
            case NativeRadar.PROXIMITY_NEAR:
                return Color.parseColor("#4CC2FF");
            case NativeRadar.PROXIMITY_MID:
                return Color.parseColor("#39D98A");
            default:
                return Color.parseColor("#7E9C90");
        }
    }

    private static int withAlpha(int color, int alpha) {
        return Color.argb(
                Math.max(0, Math.min(255, alpha)),
                Color.red(color), Color.green(color), Color.blue(color));
    }

    private static String formatMetres(double metres) {
        if (!Double.isFinite(metres)) {
            return "—";
        }
        return Math.round(metres) + "m";
    }
}
