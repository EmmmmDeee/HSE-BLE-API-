package com.hse.bleradar;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Matrix;
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
 * that dim toward the centre, a solid sweep needle, and colour-coded device
 * blips — see {@code res/drawable/ic_launcher_foreground.xml}'s doc comment
 * for exactly what was extracted) but drawn full size on a live
 * {@link Canvas} rather than as a static icon: a soft vignette backdrop,
 * faint bearing spokes, range rings with distance labels, a crisp rotating
 * sweep needle over a soft trailing wedge, and a glowing blip per tracked
 * {@link Blip}, colour-coded by {@code NativeRadar.PROXIMITY_*}.
 *
 * <p>Ownership: {@link MainActivity} pushes the latest snapshot of tracked
 * devices via {@link #setBlips(List)}; this view owns only the sweep-angle
 * animation clock and pure rendering, so it never blocks on BLE I/O.
 *
 * <p>Performance: every gradient-shaded element (per-blip glow, the centre
 * "you are here" glow) reuses one of {@link #unitGlowShaders} — a fixed,
 * radius-1 {@link RadialGradient} built once per proximity colour in
 * {@link #buildUnitGlowShaders()} — repositioned per draw with
 * {@link Shader#setLocalMatrix}, instead of constructing a brand new
 * {@link Paint} and {@link RadialGradient} for every visible device on
 * every animation tick. Rebuilding a gradient ramp is one of the more
 * expensive things a software/GPU rasteriser does per draw call, so this
 * matters directly for the low-end hardware this app targets.
 */
public final class RadarView extends View {

    private static final float SWEEP_ARC_DEGREES = 50f;
    private static final long SWEEP_PERIOD_MILLIS = 4200L;
    private static final long BLIP_FRESHNESS_WINDOW_MILLIS = 9000L;
    private static final int RING_COUNT = 4;
    private static final int SPOKE_COUNT = 8;
    private static final int PROXIMITY_COLOUR_COUNT = 4;
    private static final double RANGE_EXPAND_THRESHOLD = 1.15;
    private static final double RANGE_CONTRACT_THRESHOLD = 0.70;

    private static final int COLOUR_ACCENT = 0xFF39D98A;
    private static final int COLOUR_TEXT_PRIMARY = 0xFFE7F3ED;
    private static final int COLOUR_TEXT_SECONDARY = 0xFF7E9C90;
    private static final int COLOUR_BACKGROUND_PRIMARY = 0xFF0F1419;
    private static final int COLOUR_BACKGROUND_SECONDARY = 0xFF162026;
    private static final int COLOUR_BORDER_LIGHT = 0xB3E7F3ED;
    private static final int COLOUR_VIGNETTE_OUTER = 0xFF17232B;
    private static final int COLOUR_VIGNETTE_INNER = 0xFF0F1419;
    private static final int COLOUR_SWEEP_TRAILING = 0x0039D98A;
    private static final int COLOUR_SWEEP_LEADING = 0xA039D98A;

    private static final int VIGNETTE_NOT_YET_COMPUTED = -1;

    private double maxRangeMetres = 40.0;
    private List<Blip> blips = new ArrayList<>();
    private final long animationStartUptimeMillis = SystemClock.uptimeMillis();

    private final Paint spokePaint = strokePaint(COLOUR_ACCENT);
    private final Paint ringPaint = strokePaint(COLOUR_ACCENT);
    private final Paint rimPaint = strokePaint(COLOUR_ACCENT);
    private final Paint centrePaint = solidPaint(COLOUR_ACCENT);
    private final Paint uncertaintyPaint = strokePaint(COLOUR_BORDER_LIGHT);
    private final Paint ringLabelPaint = textPaint(COLOUR_TEXT_SECONDARY, 11f);
    private final Paint ringLabelBackgroundPaint = solidPaint(0xCC0F1419);
    private final Paint blipLabelPaint = textPaint(COLOUR_TEXT_PRIMARY, 12f);
    private final Paint blipLabelBackgroundPaint = solidPaint(0xCC162026);
    private final Paint sweepPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint needlePaint = strokePaint(0xFFEAFFF4);
    private final Paint glowPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint corePaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF sweepBounds = new RectF();
    private final Matrix glowMatrix = new Matrix();
    private final Shader[] unitGlowShaders = buildUnitGlowShaders();

    private Paint backgroundVignettePaint;
    private int vignetteWidth = VIGNETTE_NOT_YET_COMPUTED;
    private int vignetteHeight = VIGNETTE_NOT_YET_COMPUTED;

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

    /**
     * Replaces the rendered device snapshot. Safe to call from the main thread only.
     *
     * @param blips the device snapshot, or null (treated as an empty list)
     */
    public void setBlips(List<Blip> blips) {
        this.blips = blips == null ? new ArrayList<>() : new ArrayList<>(blips);
        updateAutoRange(autoRangeMetres(this.blips));
        invalidate();
    }

    private void updateAutoRange(double targetRangeMetres) {
        if (!Double.isFinite(targetRangeMetres) || targetRangeMetres <= 0.0) {
            return;
        }
        if (targetRangeMetres > maxRangeMetres * RANGE_EXPAND_THRESHOLD
                || targetRangeMetres < maxRangeMetres * RANGE_CONTRACT_THRESHOLD) {
            maxRangeMetres = targetRangeMetres;
        }
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
        paint.setStrokeCap(Paint.Cap.ROUND);
        paint.setStrokeJoin(Paint.Join.ROUND);
        return paint;
    }

    private static Paint textPaint(int color, float spSize) {
        Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        paint.setColor(color);
        paint.setTextSize(spSize * 3f);
        paint.setTextAlign(Paint.Align.CENTER);
        return paint;
    }

    /**
     * Builds one radius-1, centre-(0,0) glow gradient per {@code
     * NativeRadar.PROXIMITY_*} ordinal, with a full-alpha inner stop and a
     * zero-alpha outer stop. {@link #drawGlow} repositions/rescales one of
     * these onto each blip's actual screen position via
     * {@link Shader#setLocalMatrix} and controls the visible peak alpha
     * with plain {@link Paint#setAlpha(int)} (which multiplies the
     * shader's own per-pixel alpha — since the inner stop here is already
     * opaque, the paint alpha alone becomes the effective peak), so no
     * gradient is ever rebuilt after startup.
     */
    private static Shader[] buildUnitGlowShaders() {
        Shader[] shaders = new Shader[PROXIMITY_COLOUR_COUNT];
        for (int proximity = 0; proximity < PROXIMITY_COLOUR_COUNT; proximity++) {
            int color = colourForProximity(proximity);
            shaders[proximity] = new RadialGradient(
                    0f, 0f, 1f,
                    withAlpha(color, 255), withAlpha(color, 0),
                    Shader.TileMode.CLAMP);
        }
        return shaders;
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
        super.onSizeChanged(w, h, oldw, oldh);
        // Invalidate the cached vignette so the next onDraw rebuilds it at
        // the new size instead of stretching a stale one.
        backgroundVignettePaint = null;
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        int width = getWidth();
        int height = getHeight();
        float cx = width / 2f;
        float cy = height / 2f;
        float radius = Math.min(width, height) / 2f * 0.9f;

        drawBackground(canvas, width, height, cx, cy, radius);
        if (radius > 0) {
            drawSpokes(canvas, cx, cy, radius);
            drawRings(canvas, cx, cy, radius);
            drawSweep(canvas, cx, cy, radius);
            drawBlips(canvas, cx, cy, radius);
            drawCentreMarker(canvas, cx, cy, radius);
        }

        // Scheduled unconditionally (not only on the radius > 0 path): a
        // zero-size first pass — plausible before the parent layout's
        // weighted height resolves — must not stall the sweep animation
        // forever; the next frame will simply have a proper size.
        postInvalidateOnAnimation();
    }

    private void drawBackground(Canvas canvas, int width, int height, float cx, float cy, float radius) {
        if (backgroundVignettePaint == null || width != vignetteWidth || height != vignetteHeight) {
            vignetteWidth = width;
            vignetteHeight = height;
            float shaderRadius = Math.max(1f, radius * 1.15f);
            Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
            paint.setShader(new RadialGradient(
                    cx, cy, shaderRadius,
                    COLOUR_VIGNETTE_OUTER, COLOUR_VIGNETTE_INNER,
                    Shader.TileMode.CLAMP));
            backgroundVignettePaint = paint;
        }
        canvas.drawRect(0, 0, width, height, backgroundVignettePaint);
    }

    private void drawSpokes(Canvas canvas, float cx, float cy, float radius) {
        spokePaint.setStrokeWidth(Math.max(1f, radius * 0.004f));
        spokePaint.setAlpha(36);
        for (int i = 0; i < SPOKE_COUNT; i++) {
            double angle = Math.toRadians(i * (360.0 / SPOKE_COUNT));
            float ex = cx + radius * (float) Math.cos(angle);
            float ey = cy + radius * (float) Math.sin(angle);
            canvas.drawLine(cx, cy, ex, ey, spokePaint);
        }
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

            String label = formatMetres(maxRangeMetres * fraction);
            float labelWidth = ringLabelPaint.measureText(label) + 10f;
            float baselineY = cy - radius * fraction + ringLabelPaint.getTextSize();
            canvas.drawRoundRect(
                    cx - labelWidth / 2f, baselineY - ringLabelPaint.getTextSize(),
                    cx + labelWidth / 2f, baselineY + 3f,
                    5f, 5f, ringLabelBackgroundPaint);
            canvas.drawText(label, cx, baselineY, ringLabelPaint);
        }

        // Crisp outer bezel: a brighter thin ring exactly at the display's
        // boundary, cleanly separating the scannable disc from the
        // vignette beyond it.
        rimPaint.setAlpha(220);
        rimPaint.setStrokeWidth(Math.max(1.5f, radius * 0.01f));
        canvas.drawCircle(cx, cy, radius, rimPaint);
    }

    private void drawSweep(Canvas canvas, float cx, float cy, float radius) {
        long elapsed = SystemClock.uptimeMillis() - animationStartUptimeMillis;
        float sweepAngle = (elapsed % SWEEP_PERIOD_MILLIS) / (float) SWEEP_PERIOD_MILLIS * 360f;

        sweepBounds.set(cx - radius, cy - radius, cx + radius, cy + radius);
        SweepGradient shader = new SweepGradient(
                cx, cy,
                new int[] {COLOUR_SWEEP_TRAILING, COLOUR_SWEEP_TRAILING, COLOUR_SWEEP_LEADING},
                new float[] {0f, 1f - (SWEEP_ARC_DEGREES / 360f), 1f});
        sweepPaint.setShader(shader);
        sweepPaint.setStyle(Paint.Style.FILL);
        canvas.save();
        canvas.rotate(sweepAngle - SWEEP_ARC_DEGREES, cx, cy);
        canvas.drawArc(sweepBounds, 0f, 360f, true, sweepPaint);
        canvas.restore();

        needlePaint.setStrokeWidth(Math.max(2f, radius * 0.018f));
        needlePaint.setAlpha(235);
        double needleAngle = Math.toRadians(sweepAngle - 90.0);
        float nx = cx + radius * (float) Math.cos(needleAngle);
        float ny = cy + radius * (float) Math.sin(needleAngle);
        canvas.drawLine(cx, cy, nx, ny, needlePaint);
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
            double angleRad = Math.toRadians(blip.angleDegrees - 90.0);
            double rangeStart = Double.isFinite(blip.distanceLowerBoundMetres)
                    ? blip.distanceLowerBoundMetres
                    : distance;
            double rangeEnd = Double.isFinite(blip.distanceUpperBoundMetres)
                    ? blip.distanceUpperBoundMetres
                    : distance;
            float startNormalised = normaliseRange(rangeStart);
            float endNormalised = normaliseRange(rangeEnd);
            float centerNormalised = normaliseRange(distance);
            float startX = cx + radius * startNormalised * (float) Math.cos(angleRad);
            float startY = cy + radius * startNormalised * (float) Math.sin(angleRad);
            float endX = cx + radius * endNormalised * (float) Math.cos(angleRad);
            float endY = cy + radius * endNormalised * (float) Math.sin(angleRad);
            float bx = cx + radius * centerNormalised * (float) Math.cos(angleRad);
            float by = cy + radius * centerNormalised * (float) Math.sin(angleRad);

            long age = now - blip.lastSeenUptimeMillis;
            float freshness = 1f - Math.min(1f, age / (float) BLIP_FRESHNESS_WINDOW_MILLIS);
            int blipColor = colourForProximity(blip.proximity);
            float confidence = Math.max(0f, Math.min(100f, blip.confidencePercent)) / 100f;
            int glowAlpha = Math.round((90f + 165f * confidence) * freshness);
            int coreAlpha = Math.round(255 * (0.55f + 0.45f * freshness));
            float blipRadius = Math.max(5f, radius * 0.035f);

            if (endNormalised > startNormalised + 0.01f) {
                uncertaintyPaint.setStrokeWidth(Math.max(2f, blipRadius * (0.45f + 0.25f * confidence)));
                uncertaintyPaint.setColor(withAlpha(blipColor,
                        Math.round((120f + 100f * confidence) * freshness)));
                canvas.drawLine(startX, startY, endX, endY, uncertaintyPaint);

                // Endpoint markers make the range legible even when the
                // center blip is moving or the range is wider than the core.
                corePaint.setStyle(Paint.Style.STROKE);
                corePaint.setStrokeWidth(Math.max(1f, blipRadius * 0.16f));
                corePaint.setColor(withAlpha(blipColor, Math.round(150 * freshness)));
                float endpointRadius = Math.max(2f, blipRadius * 0.38f);
                canvas.drawCircle(startX, startY, endpointRadius, corePaint);
                canvas.drawCircle(endX, endY, endpointRadius, corePaint);
            }
            drawGlow(canvas, blip.proximity, bx, by, blipRadius * 3.2f, glowAlpha);

            corePaint.setStyle(Paint.Style.FILL);
            corePaint.setColor(withAlpha(blipColor, coreAlpha));
            canvas.drawCircle(bx, by, blipRadius, corePaint);
            // Bright rim highlight on the core, distinguishing a live blip
            // from the flat ring strokes behind it.
            corePaint.setStyle(Paint.Style.STROKE);
            corePaint.setStrokeWidth(Math.max(1f, blipRadius * (0.16f + 0.18f * confidence)));
            corePaint.setColor(withAlpha(Color.WHITE, Math.round((65f + 120f * confidence) * freshness)));
            canvas.drawCircle(bx, by, blipRadius, corePaint);

            String label = shortLabel(blip);
            float labelWidth = blipLabelPaint.measureText(label) + 12f;
            float labelHeight = blipLabelPaint.getTextSize() + 8f;
            float labelCenterX = clamp(
                    bx,
                    cx - radius + labelWidth / 2f + 4f,
                    cx + radius - labelWidth / 2f - 4f);
            float labelTop = by + blipRadius + 2f;
            if (labelTop + labelHeight > cy + radius) {
                labelTop = by - blipRadius - labelHeight - 2f;
            }
            labelTop = clamp(labelTop, cy - radius + 2f, cy + radius - labelHeight - 2f);
            canvas.drawRoundRect(
                    labelCenterX - labelWidth / 2f, labelTop,
                    labelCenterX + labelWidth / 2f, labelTop + labelHeight,
                    6f, 6f, blipLabelBackgroundPaint);
            canvas.drawText(label, labelCenterX, labelTop + blipLabelPaint.getTextSize() + 2f, blipLabelPaint);
        }
    }

    private void drawCentreMarker(Canvas canvas, float cx, float cy, float radius) {
        long elapsed = SystemClock.uptimeMillis() - animationStartUptimeMillis;
        // Slow, subtle breathing pulse (~1.8s period) so the "you are
        // here" marker reads as alive rather than static, without being
        // distracting.
        float pulse = 0.5f + 0.5f * (float) Math.sin(elapsed / 900.0);
        float centreRadius = Math.max(4f, radius * 0.02f);
        drawGlow(canvas, NativeRadar.PROXIMITY_MID, cx, cy, centreRadius * 4.5f, Math.round(70 + 50 * pulse));
        canvas.drawCircle(cx, cy, centreRadius, centrePaint);
    }

    /**
     * Repositions and rescales the cached unit gradient for {@code
     * proximity} onto {@code (cx, cy)} at {@code glowRadius}, with a peak
     * alpha of {@code alpha}; see {@link #buildUnitGlowShaders()} for why
     * this never allocates a new {@link Paint} or {@link Shader}.
     */
    private void drawGlow(Canvas canvas, int proximity, float cx, float cy, float glowRadius, int alpha) {
        if (glowRadius <= 0f || alpha <= 0) {
            return;
        }
        int index = proximity >= 0 && proximity < unitGlowShaders.length
                ? proximity
                : NativeRadar.PROXIMITY_FAR;
        glowMatrix.setScale(glowRadius, glowRadius);
        glowMatrix.postTranslate(cx, cy);
        Shader shader = unitGlowShaders[index];
        shader.setLocalMatrix(glowMatrix);
        glowPaint.setShader(shader);
        glowPaint.setAlpha(Math.max(0, Math.min(255, alpha)));
        canvas.drawCircle(cx, cy, glowRadius, glowPaint);
    }

    private static String shortLabel(Blip blip) {
        if (blip.name != null && !blip.name.isEmpty()) {
            return blip.name;
        }
        String address = blip.address;
        return address.length() > 5 ? address.substring(address.length() - 5) : address;
    }

    private float normaliseRange(double metres) {
        return (float) Math.min(1.0, Math.max(0.0, metres / maxRangeMetres));
    }

    private static float clamp(float value, float lower, float upper) {
        if (lower > upper) {
            return (lower + upper) / 2f;
        }
        return Math.max(lower, Math.min(upper, value));
    }

    private static double autoRangeMetres(List<Blip> blips) {
        double strongestBound = 0.0;
        long now = SystemClock.uptimeMillis();
        for (Blip blip : blips) {
            if (!blip.isFresh(now, BLIP_FRESHNESS_WINDOW_MILLIS)) {
                continue;
            }
            double candidate = Double.isFinite(blip.distanceUpperBoundMetres)
                    ? blip.distanceUpperBoundMetres
                    : Double.isFinite(blip.distanceMetres)
                    ? blip.distanceMetres
                    : fallbackDistanceForProximity(blip.proximity);
            strongestBound = Math.max(strongestBound, candidate);
        }
        if (strongestBound <= 0.0 || !Double.isFinite(strongestBound)) {
            return 40.0;
        }
        double padded = strongestBound * 1.2;
        double clamped = Math.min(60.0, Math.max(12.0, padded));
        return Math.ceil(clamped / 5.0) * 5.0;
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

    /** Package-visible so {@link MainActivity}'s device list can mirror the radar's own palette. */
    static int colourForProximity(int proximity) {
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
