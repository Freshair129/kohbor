param(
    [string]$ReferencePath,
    [string]$CandidatePath,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$source = @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class KohborOverviewRegistration {
    private const int SourceSide = 1024;
    private const int Side = 512;

    public sealed class AxisTransform {
        public double scale { get; set; }
        public double offset_px { get; set; }
    }

    public sealed class PointResidual {
        public string point { get; set; }
        public double candidate_x_px { get; set; }
        public double candidate_y_px { get; set; }
        public double reference_x_px { get; set; }
        public double reference_y_px { get; set; }
        public double official_delta_x_px { get; set; }
        public double official_delta_y_px { get; set; }
        public double official_delta_length_px { get; set; }
    }

    public sealed class MeasuredFeature {
        public string measurement { get; set; }
        public int pixel_count { get; set; }
        public double center_x_px { get; set; }
        public double center_y_px { get; set; }
        public int min_x_px { get; set; }
        public int min_y_px { get; set; }
        public int max_x_px { get; set; }
        public int max_y_px { get; set; }
    }

    public sealed class LandmarkCheck {
        public string name { get; set; }
        public double world_x { get; set; }
        public double world_y { get; set; }
        public double official_reference_x_px { get; set; }
        public double official_reference_y_px { get; set; }
        public double candidate_projected_x_px { get; set; }
        public double candidate_projected_y_px { get; set; }
        public double measured_candidate_center_x_px { get; set; }
        public double measured_candidate_center_y_px { get; set; }
        public double feature_center_residual_px { get; set; }
        public bool projection_inside_measured_feature { get; set; }
        public bool? candidate_projected_pixel_matches_feature_mask { get; set; }
        public MeasuredFeature reference_feature { get; set; }
        public MeasuredFeature candidate_feature { get; set; }
    }

    public sealed class RegistrationResult {
        public string method { get; set; }
        public int image_width { get; set; }
        public int image_height { get; set; }
        public AxisTransform x_candidate_to_reference { get; set; }
        public AxisTransform y_candidate_to_reference { get; set; }
        public double identity_edge_correlation { get; set; }
        public double fitted_edge_correlation { get; set; }
        public List<PointResidual> residuals { get; set; }
        public List<LandmarkCheck> landmark_checks { get; set; }
        public bool? self_test_passed { get; set; }
    }

    private sealed class Edges {
        public float[] magnitude;
        public float[] gx;
        public float[] gy;
        public float[] profile_x;
        public float[] profile_y;
    }

    private sealed class AxisFit {
        public double scale;
        public double offset;
        public double score;
    }

    public static RegistrationResult Register(string referencePath, string candidatePath) {
        float[] reference = ReadGray512(referencePath);
        float[] candidate = ReadGray512(candidatePath);
        RegistrationResult result = Fit(reference, candidate, false);
        if (result.fitted_edge_correlation <= result.identity_edge_correlation) {
            throw new InvalidOperationException("The fitted edge registration did not improve on the official identity mapping.");
        }
        result.landmark_checks = MeasureLandmarks(referencePath, candidatePath, result);
        if (result.landmark_checks.Exists(x => !x.projection_inside_measured_feature)) {
            throw new InvalidOperationException("A projected map landmark fell outside its measured art feature.");
        }
        return result;
    }

    public static RegistrationResult SelfTest() {
        const double expectedSx = 1.0123;
        const double expectedOx = -3.5;
        const double expectedSy = 0.9865;
        const double expectedOy = 2.25;
        float[] reference = MakePattern();
        float[] candidate = new float[Side * Side];
        for (int y = 0; y < Side; y++) {
            for (int x = 0; x < Side; x++) {
                candidate[y * Side + x] = Bilinear(reference, Side,
                    expectedSx * x + expectedOx, expectedSy * y + expectedOy);
            }
        }
        RegistrationResult result = Fit(reference, candidate, true);
        double xScaleError = Math.Abs(result.x_candidate_to_reference.scale - expectedSx) * 1023.0;
        double yScaleError = Math.Abs(result.y_candidate_to_reference.scale - expectedSy) * 1023.0;
        double xOffsetError = Math.Abs(result.x_candidate_to_reference.offset_px - expectedOx * 2.0);
        double yOffsetError = Math.Abs(result.y_candidate_to_reference.offset_px - expectedOy * 2.0);
        result.self_test_passed = xScaleError <= 1.0 && yScaleError <= 1.0
            && xOffsetError <= 1.0 && yOffsetError <= 1.0;
        if (!result.self_test_passed.GetValueOrDefault()) {
            throw new InvalidOperationException(String.Format(
                "Synthetic affine registration failed: scale errors {0:F3}/{1:F3}px, offsets {2:F3}/{3:F3}px",
                xScaleError, yScaleError, xOffsetError, yOffsetError));
        }
        return result;
    }

    private static float[] ReadGray512(string path) {
        using (Bitmap original = new Bitmap(path)) {
            if (original.Width != SourceSide || original.Height != SourceSide) {
                throw new InvalidOperationException(String.Format(
                    "Expected a {0}x{0} PNG: {1} is {2}x{3}", SourceSide, path, original.Width, original.Height));
            }
            using (Bitmap bitmap = new Bitmap(SourceSide, SourceSide, PixelFormat.Format32bppArgb)) {
                using (Graphics graphics = Graphics.FromImage(bitmap)) {
                    graphics.DrawImage(original, new Rectangle(0, 0, SourceSide, SourceSide));
                }
                Rectangle rect = new Rectangle(0, 0, SourceSide, SourceSide);
                BitmapData data = bitmap.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
                try {
                    int stride = Math.Abs(data.Stride);
                    byte[] bytes = new byte[stride * SourceSide];
                    Marshal.Copy(data.Scan0, bytes, 0, bytes.Length);
                    float[] gray = new float[Side * Side];
                    for (int y = 0; y < Side; y++) {
                        for (int x = 0; x < Side; x++) {
                            double sum = 0;
                            for (int dy = 0; dy < 2; dy++) {
                                int row = (y * 2 + dy) * stride;
                                for (int dx = 0; dx < 2; dx++) {
                                    int i = row + (x * 2 + dx) * 4;
                                    double alpha = bytes[i + 3] / 255.0;
                                    sum += (0.2126 * bytes[i + 2] + 0.7152 * bytes[i + 1]
                                        + 0.0722 * bytes[i]) * alpha;
                                }
                            }
                            gray[y * Side + x] = (float)(sum / 4.0);
                        }
                    }
                    return gray;
                } finally {
                    bitmap.UnlockBits(data);
                }
            }
        }
    }

    private static List<LandmarkCheck> MeasureLandmarks(string referencePath, string candidatePath,
        RegistrationResult fit) {
        const double posX = -9472.0, posY = 9472.0, scale = 18.5;
        var checks = new List<LandmarkCheck>();
        MeasuredFeature fountainReference = MeasureFountain(referencePath);
        MeasuredFeature fountainCandidate = MeasureFountain(candidatePath);
        double fountainWorldX = fountainReference.center_x_px * scale + posX;
        double fountainWorldY = posY - fountainReference.center_y_px * scale;
        double fountainCandidateX = ToCandidateX(fountainReference.center_x_px, fit);
        double fountainCandidateY = ToCandidateY(fountainReference.center_y_px, fit);
        checks.Add(MakeLandmark("radiant_fountain_feature_centroid", fountainWorldX, fountainWorldY,
            fountainReference.center_x_px, fountainReference.center_y_px,
            fountainReference, fountainCandidate, fountainCandidateX, fountainCandidateY));

        const double tormentorWorldX = 7744.0, tormentorWorldY = -6208.0;
        double tormentorRefX = (tormentorWorldX - posX) / scale;
        double tormentorRefY = (posY - tormentorWorldY) / scale;
        double tormentorCandidateX = ToCandidateX(tormentorRefX, fit);
        double tormentorCandidateY = ToCandidateY(tormentorRefY, fit);
        MeasuredFeature tormentorReference = FindTormentorFeature(referencePath, tormentorRefX, tormentorRefY);
        MeasuredFeature tormentorCandidate = FindTormentorFeature(candidatePath, tormentorCandidateX, tormentorCandidateY);
        LandmarkCheck tormentor = MakeLandmark("tormentor_7744_-6208", tormentorWorldX, tormentorWorldY,
            tormentorRefX, tormentorRefY, tormentorReference, tormentorCandidate,
            tormentorCandidateX, tormentorCandidateY);
        tormentor.candidate_projected_pixel_matches_feature_mask = IsNeutralFeaturePixel(
            candidatePath, tormentorCandidateX, tormentorCandidateY);
        checks.Add(tormentor);
        return checks;
    }

    private static LandmarkCheck MakeLandmark(string name, double worldX, double worldY,
        double referenceX, double referenceY, MeasuredFeature referenceFeature,
        MeasuredFeature candidateFeature,
        double projectedCandidateX, double projectedCandidateY) {
        double dx = projectedCandidateX - candidateFeature.center_x_px;
        double dy = projectedCandidateY - candidateFeature.center_y_px;
        bool inside = projectedCandidateX >= candidateFeature.min_x_px
            && projectedCandidateX <= candidateFeature.max_x_px
            && projectedCandidateY >= candidateFeature.min_y_px
            && projectedCandidateY <= candidateFeature.max_y_px;
        return new LandmarkCheck {
            name = name,
            world_x = worldX,
            world_y = worldY,
            official_reference_x_px = referenceX,
            official_reference_y_px = referenceY,
            candidate_projected_x_px = projectedCandidateX,
            candidate_projected_y_px = projectedCandidateY,
            measured_candidate_center_x_px = candidateFeature.center_x_px,
            measured_candidate_center_y_px = candidateFeature.center_y_px,
            feature_center_residual_px = Math.Sqrt(dx * dx + dy * dy),
            projection_inside_measured_feature = inside,
            reference_feature = referenceFeature,
            candidate_feature = candidateFeature
        };
    }

    private static double ToCandidateX(double referenceX, RegistrationResult fit) {
        AxisTransform axis = fit.x_candidate_to_reference;
        return (referenceX - axis.offset_px) / axis.scale;
    }

    private static double ToCandidateY(double referenceY, RegistrationResult fit) {
        AxisTransform axis = fit.y_candidate_to_reference;
        return (referenceY - axis.offset_px) / axis.scale;
    }

    private static MeasuredFeature MeasureFountain(string path) {
        const int x0 = 50, y0 = 820, width = 100, height = 110;
        int count = 0, sumX = 0, sumY = 0;
        int minX = 1024, minY = 1024, maxX = 0, maxY = 0;
        using (Bitmap bitmap = new Bitmap(path)) {
            for (int y = y0; y < y0 + height; y++) {
                for (int x = x0; x < x0 + width; x++) {
                    Color color = bitmap.GetPixel(x, y);
                    if (color.B <= 105 || color.G <= 75 || color.B <= color.R + 18 || color.G <= color.R + 10)
                        continue;
                    count++;
                    sumX += x; sumY += y;
                    minX = Math.Min(minX, x); minY = Math.Min(minY, y);
                    maxX = Math.Max(maxX, x); maxY = Math.Max(maxY, y);
                }
            }
        }
        if (count == 0) throw new InvalidOperationException("No fountain-colored art pixels were measured in the reference ROI.");
        return new MeasuredFeature {
            measurement = "centroid of cyan/blue pixels (B>105, G>75, B>R+18, G>R+10) in x=50..149,y=820..929",
            pixel_count = count,
            center_x_px = (double)sumX / count,
            center_y_px = (double)sumY / count,
            min_x_px = minX, min_y_px = minY, max_x_px = maxX, max_y_px = maxY
        };
    }

    private static MeasuredFeature FindTormentorFeature(string path, double centerX, double centerY) {
        int x0 = Math.Max(0, (int)Math.Floor(centerX - 36));
        int y0 = Math.Max(0, (int)Math.Floor(centerY - 36));
        int x1 = Math.Min(1023, (int)Math.Ceiling(centerX + 36));
        int y1 = Math.Min(1023, (int)Math.Ceiling(centerY + 36));
        int width = x1 - x0 + 1, height = y1 - y0 + 1;
        bool[] mask = new bool[width * height];
        using (Bitmap bitmap = new Bitmap(path)) {
            for (int y = 0; y < height; y++) {
                for (int x = 0; x < width; x++) {
                    Color color = bitmap.GetPixel(x0 + x, y0 + y);
                    int lo = Math.Min(color.R, Math.Min(color.G, color.B));
                    int hi = Math.Max(color.R, Math.Max(color.G, color.B));
                    mask[y * width + x] = lo > 105 && hi - lo < 90;
                }
            }
        }
        bool[] seen = new bool[mask.Length];
        MeasuredFeature best = null;
        long bestBoxArea = long.MaxValue;
        for (int i = 0; i < mask.Length; i++) {
            if (!mask[i] || seen[i]) continue;
            var queue = new Queue<int>();
            queue.Enqueue(i); seen[i] = true;
            int count = 0, sumX = 0, sumY = 0;
            int minX = width, minY = height, maxX = 0, maxY = 0;
            while (queue.Count > 0) {
                int current = queue.Dequeue();
                int x = current % width, y = current / width;
                count++; sumX += x0 + x; sumY += y0 + y;
                minX = Math.Min(minX, x); minY = Math.Min(minY, y);
                maxX = Math.Max(maxX, x); maxY = Math.Max(maxY, y);
                for (int dy = -1; dy <= 1; dy++) {
                    for (int dx = -1; dx <= 1; dx++) {
                        int nx = x + dx, ny = y + dy;
                        if (nx < 0 || nx >= width || ny < 0 || ny >= height) continue;
                        int next = ny * width + nx;
                        if (mask[next] && !seen[next]) { seen[next] = true; queue.Enqueue(next); }
                    }
                }
            }
            int globalMinX = x0 + minX, globalMinY = y0 + minY;
            int globalMaxX = x0 + maxX, globalMaxY = y0 + maxY;
            bool contains = centerX >= globalMinX && centerX <= globalMaxX
                && centerY >= globalMinY && centerY <= globalMaxY;
            long boxArea = (long)(globalMaxX - globalMinX + 1) * (globalMaxY - globalMinY + 1);
            if (count < 50 || !contains || boxArea >= bestBoxArea) continue;
            bestBoxArea = boxArea;
            best = new MeasuredFeature {
                measurement = "smallest connected near-neutral bright component containing the projected point (min RGB>105, channel range<90)",
                pixel_count = count,
                center_x_px = (double)sumX / count,
                center_y_px = (double)sumY / count,
                min_x_px = globalMinX, min_y_px = globalMinY,
                max_x_px = globalMaxX, max_y_px = globalMaxY
            };
        }
        if (best == null) throw new InvalidOperationException("No bright Tormentor art feature contains the projected coordinate.");
        return best;
    }

    private static bool IsNeutralFeaturePixel(string path, double x, double y) {
        int px = Math.Max(0, Math.Min(1023, (int)Math.Round(x)));
        int py = Math.Max(0, Math.Min(1023, (int)Math.Round(y)));
        using (Bitmap bitmap = new Bitmap(path)) {
            Color color = bitmap.GetPixel(px, py);
            int lo = Math.Min(color.R, Math.Min(color.G, color.B));
            int hi = Math.Max(color.R, Math.Max(color.G, color.B));
            return lo > 105 && hi - lo < 90;
        }
    }

    private static float[] MakePattern() {
        float[] pixels = new float[Side * Side];
        Random random = new Random(742028);
        for (int i = 0; i < pixels.Length; i++) pixels[i] = 18 + random.Next(0, 7);
        for (int n = 0; n < 90; n++) {
            int left = random.Next(24, 448);
            int top = random.Next(24, 448);
            int width = random.Next(8, 48);
            int height = random.Next(8, 48);
            float value = random.Next(55, 240);
            for (int y = top; y < Math.Min(Side, top + height); y++) {
                for (int x = left; x < Math.Min(Side, left + width); x++) {
                    if (x == left || x == left + width - 1 || y == top || y == top + height - 1)
                        pixels[y * Side + x] = value;
                }
            }
        }
        for (int n = 0; n < 28; n++) {
            int x0 = random.Next(32, 440);
            int y0 = random.Next(32, 440);
            int x1 = random.Next(32, 440);
            int y1 = random.Next(32, 440);
            DrawLine(pixels, x0, y0, x1, y1, random.Next(80, 250));
        }
        return pixels;
    }

    private static void DrawLine(float[] pixels, int x0, int y0, int x1, int y1, float value) {
        int dx = Math.Abs(x1 - x0), sx = x0 < x1 ? 1 : -1;
        int dy = -Math.Abs(y1 - y0), sy = y0 < y1 ? 1 : -1;
        int error = dx + dy;
        while (true) {
            pixels[y0 * Side + x0] = value;
            if (x0 == x1 && y0 == y1) break;
            int twice = 2 * error;
            if (twice >= dy) { error += dy; x0 += sx; }
            if (twice <= dx) { error += dx; y0 += sy; }
        }
    }

    private static RegistrationResult Fit(float[] reference, float[] candidate, bool selfTest) {
        Edges r = BuildEdges(reference);
        Edges c = BuildEdges(candidate);
        AxisFit x = SearchProfile(r.profile_x, c.profile_x);
        AxisFit y = SearchProfile(r.profile_y, c.profile_y);
        double identityScore = Correlation2D(r.magnitude, c.magnitude, 1, 0, 1, 0);

        for (int pass = 0; pass < 2; pass++) {
            SearchAxisPair(r.magnitude, c.magnitude, true, ref x.scale, ref x.offset,
                y.scale, y.offset, 0.02, 8, 0.001, 0.5);
            SearchAxisPair(r.magnitude, c.magnitude, false, ref y.scale, ref y.offset,
                x.scale, x.offset, 0.02, 8, 0.001, 0.5);
        }
        SearchAxisPair(r.magnitude, c.magnitude, true, ref x.scale, ref x.offset,
            y.scale, y.offset, 0.0012, 0.6, 0.0001, 0.05);
        SearchAxisPair(r.magnitude, c.magnitude, false, ref y.scale, ref y.offset,
            x.scale, x.offset, 0.0012, 0.6, 0.0001, 0.05);
        double fittedScore = Correlation2D(r.magnitude, c.magnitude, x.scale, x.offset, y.scale, y.offset);

        return new RegistrationResult {
            method = "Sobel edge cross-correlation; axis-aligned affine fit from candidate pixels to official-reference pixels",
            image_width = SourceSide,
            image_height = SourceSide,
            x_candidate_to_reference = new AxisTransform { scale = x.scale, offset_px = x.offset * 2.0 },
            y_candidate_to_reference = new AxisTransform { scale = y.scale, offset_px = y.offset * 2.0 },
            identity_edge_correlation = identityScore,
            fitted_edge_correlation = fittedScore,
            residuals = MakeResiduals(x.scale, x.offset * 2.0, y.scale, y.offset * 2.0)
        };
    }

    private static Edges BuildEdges(float[] gray) {
        Edges result = new Edges {
            magnitude = new float[Side * Side],
            gx = new float[Side * Side],
            gy = new float[Side * Side],
            profile_x = new float[Side],
            profile_y = new float[Side]
        };
        for (int y = 1; y < Side - 1; y++) {
            for (int x = 1; x < Side - 1; x++) {
                int i = y * Side + x;
                float tl = gray[i - Side - 1], t = gray[i - Side], tr = gray[i - Side + 1];
                float l = gray[i - 1], rr = gray[i + 1];
                float bl = gray[i + Side - 1], b = gray[i + Side], br = gray[i + Side + 1];
                float gx = -tl + tr - 2 * l + 2 * rr - bl + br;
                float gy = -tl - 2 * t - tr + bl + 2 * b + br;
                result.gx[i] = gx;
                result.gy[i] = gy;
                result.magnitude[i] = (float)Math.Sqrt(gx * gx + gy * gy);
                result.profile_x[x] += Math.Abs(gx);
                result.profile_y[y] += Math.Abs(gy);
            }
        }
        return result;
    }

    private static AxisFit SearchProfile(float[] reference, float[] candidate) {
        AxisFit best = new AxisFit { scale = 1, offset = 0, score = -2 };
        for (int si = 0; si <= 150; si++) {
            double scale = 0.85 + si * 0.002;
            for (int oi = -128; oi <= 128; oi++) {
                double offset = oi * 0.5;
                double score = Correlation1D(reference, candidate, scale, offset);
                if (score > best.score) best = new AxisFit { scale = scale, offset = offset, score = score };
            }
        }
        AxisFit coarse = best;
        for (int si = -60; si <= 60; si++) {
            double scale = coarse.scale + si * 0.0001;
            for (int oi = -40; oi <= 40; oi++) {
                double offset = coarse.offset + oi * 0.05;
                double score = Correlation1D(reference, candidate, scale, offset);
                if (score > best.score) best = new AxisFit { scale = scale, offset = offset, score = score };
            }
        }
        return best;
    }

    private static double Correlation1D(float[] reference, float[] candidate, double scale, double offset) {
        double sumR = 0, sumC = 0, sumRR = 0, sumCC = 0, sumRC = 0;
        int count = 0;
        for (int x = 60; x <= 452; x++) {
            double sourceX = (x - offset) / scale;
            if (sourceX < 1 || sourceX > Side - 2) continue;
            double r = reference[x];
            double c = Sample1D(candidate, sourceX);
            sumR += r; sumC += c; sumRR += r * r; sumCC += c * c; sumRC += r * c;
            count++;
        }
        return Pearson(sumR, sumC, sumRR, sumCC, sumRC, count);
    }

    private static void SearchAxisPair(float[] reference, float[] candidate, bool xAxis,
        ref double scale, ref double offset, double otherScale, double otherOffset,
        double rangeScale, double rangeOffset, double stepScale, double stepOffset) {
        int scaleSteps = (int)Math.Round(rangeScale / stepScale);
        int offsetSteps = (int)Math.Round(rangeOffset / stepOffset);
        double bestScore = -2, bestScale = scale, bestOffset = offset;
        for (int si = -scaleSteps; si <= scaleSteps; si++) {
            double testScale = scale + si * stepScale;
            if (testScale <= 0) continue;
            for (int oi = -offsetSteps; oi <= offsetSteps; oi++) {
                double testOffset = offset + oi * stepOffset;
                double sx = xAxis ? testScale : otherScale;
                double ox = xAxis ? testOffset : otherOffset;
                double sy = xAxis ? otherScale : testScale;
                double oy = xAxis ? otherOffset : testOffset;
                double score = Correlation2D(reference, candidate, sx, ox, sy, oy);
                if (score > bestScore) { bestScore = score; bestScale = testScale; bestOffset = testOffset; }
            }
        }
        scale = bestScale;
        offset = bestOffset;
    }

    private static double Correlation2D(float[] reference, float[] candidate,
        double scaleX, double offsetX, double scaleY, double offsetY) {
        double sumR = 0, sumC = 0, sumRR = 0, sumCC = 0, sumRC = 0;
        int count = 0;
        for (int y = 48; y <= 464; y += 6) {
            double sourceY = (y - offsetY) / scaleY;
            if (sourceY < 1 || sourceY > Side - 2) continue;
            for (int x = 48; x <= 464; x += 6) {
                double sourceX = (x - offsetX) / scaleX;
                if (sourceX < 1 || sourceX > Side - 2) continue;
                double r = reference[y * Side + x];
                double c = Sample2D(candidate, sourceX, sourceY);
                sumR += r; sumC += c; sumRR += r * r; sumCC += c * c; sumRC += r * c;
                count++;
            }
        }
        return Pearson(sumR, sumC, sumRR, sumCC, sumRC, count);
    }

    private static double Pearson(double sumA, double sumB, double sumAA,
        double sumBB, double sumAB, int count) {
        if (count < 100) return -2;
        double varA = sumAA - sumA * sumA / count;
        double varB = sumBB - sumB * sumB / count;
        if (varA <= 1e-9 || varB <= 1e-9) return -2;
        return (sumAB - sumA * sumB / count) / Math.Sqrt(varA * varB);
    }

    private static double Sample1D(float[] values, double x) {
        int x0 = (int)Math.Floor(x), x1 = x0 + 1;
        double t = x - x0;
        return values[x0] * (1 - t) + values[x1] * t;
    }

    private static double Sample2D(float[] values, double x, double y) {
        int x0 = (int)Math.Floor(x), y0 = (int)Math.Floor(y);
        int x1 = x0 + 1, y1 = y0 + 1;
        double tx = x - x0, ty = y - y0;
        double top = values[y0 * Side + x0] * (1 - tx) + values[y0 * Side + x1] * tx;
        double bottom = values[y1 * Side + x0] * (1 - tx) + values[y1 * Side + x1] * tx;
        return top * (1 - ty) + bottom * ty;
    }

    private static float Bilinear(float[] pixels, int side, double x, double y) {
        if (x < 0 || x > side - 1 || y < 0 || y > side - 1) return 18;
        int x0 = (int)Math.Floor(x), y0 = (int)Math.Floor(y);
        int x1 = Math.Min(side - 1, x0 + 1), y1 = Math.Min(side - 1, y0 + 1);
        double tx = x - x0, ty = y - y0;
        double top = pixels[y0 * side + x0] * (1 - tx) + pixels[y0 * side + x1] * tx;
        double bottom = pixels[y1 * side + x0] * (1 - tx) + pixels[y1 * side + x1] * tx;
        return (float)(top * (1 - ty) + bottom * ty);
    }

    private static List<PointResidual> MakeResiduals(double sx, double ox, double sy, double oy) {
        var points = new List<PointResidual>();
        AddPoint(points, "top_left", 0, 0, sx, ox, sy, oy);
        AddPoint(points, "top_right", 1023, 0, sx, ox, sy, oy);
        AddPoint(points, "bottom_right", 1023, 1023, sx, ox, sy, oy);
        AddPoint(points, "bottom_left", 0, 1023, sx, ox, sy, oy);
        AddPoint(points, "center", 511.5, 511.5, sx, ox, sy, oy);
        return points;
    }

    private static void AddPoint(List<PointResidual> points, string name,
        double x, double y, double sx, double ox, double sy, double oy) {
        double referenceX = sx * x + ox;
        double referenceY = sy * y + oy;
        double dx = referenceX - x, dy = referenceY - y;
        points.Add(new PointResidual {
            point = name,
            candidate_x_px = x,
            candidate_y_px = y,
            reference_x_px = referenceX,
            reference_y_px = referenceY,
            official_delta_x_px = dx,
            official_delta_y_px = dy,
            official_delta_length_px = Math.Sqrt(dx * dx + dy * dy)
        });
    }
}
'@

$drawingAssembly = [System.Drawing.Bitmap].Assembly.Location
Add-Type -TypeDefinition $source -ReferencedAssemblies @($drawingAssembly)

if ($SelfTest) {
    $result = [KohborOverviewRegistration]::SelfTest()
} else {
    if (-not $ReferencePath -or -not $CandidatePath) {
        throw 'Provide -ReferencePath and -CandidatePath, or use -SelfTest.'
    }
    $result = [KohborOverviewRegistration]::Register(
        (Resolve-Path -LiteralPath $ReferencePath).Path,
        (Resolve-Path -LiteralPath $CandidatePath).Path
    )
}

$result | ConvertTo-Json -Depth 6
