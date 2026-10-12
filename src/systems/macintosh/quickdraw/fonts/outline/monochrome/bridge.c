/* Rust-owned source outlines and buffers; the converter owns only its context. */
#define FT_CONFIG_STANDARD_LIBRARY_H <stdlib.h>
#include "ftmisc.h"
#include "ftimage.h"
#include <string.h>

extern const FT_Raster_Funcs ft_standard_raster;

void *systemless_mono_new(void) {
    FT_Raster raster = NULL;
    if (ft_standard_raster.raster_new(NULL, &raster) != 0) return NULL;
    return raster;
}

void systemless_mono_done(void *raster) {
    ft_standard_raster.raster_done((FT_Raster)raster);
}

int systemless_mono_render(void *raster, FT_Vector *points, char *tags,
                          short *contours, short point_count, short contour_count,
                          int width, int height, unsigned char *buffer) {
    FT_Outline outline;
    FT_Bitmap bitmap;
    FT_Raster_Params params;
    int previous = -1;
    if (!raster || !points || !tags || !contours || !buffer || point_count <= 0 ||
        contour_count <= 0 || width <= 0 || height <= 0) return -1;
    for (int i = 0; i < contour_count; ++i) {
        if (contours[i] <= previous || contours[i] >= point_count) return -1;
        previous = contours[i];
    }
    if (previous != point_count - 1) return -1;
    memset(&outline, 0, sizeof(outline));
    outline.n_points = point_count; outline.n_contours = contour_count;
    outline.points = points; outline.tags = tags; outline.contours = contours;
    /* Default simple dropout control, excluding stubs. Do not invent smart
       dropout hints that are absent from the source outline contract. */
    outline.flags = 0;
    memset(&bitmap, 0, sizeof(bitmap));
    bitmap.width = width; bitmap.rows = height; bitmap.pitch = (width + 7) / 8;
    bitmap.buffer = buffer; bitmap.pixel_mode = FT_PIXEL_MODE_MONO; bitmap.num_grays = 2;
    memset(&params, 0, sizeof(params));
    params.source = &outline; params.target = &bitmap;
    return ft_standard_raster.raster_render((FT_Raster)raster, &params);
}
