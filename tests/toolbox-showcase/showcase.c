/*
 * Toolbox Showcase: Classic Macintosh Fat-App Fixture.
 * Compiled unchanged for 68K and native PowerPC.
 *
 * Exercises standard Macintosh Toolbox subsystems:
 * - Window Manager & Event Manager (Macintosh Toolbox Essentials ch 2, 4)
 * - Menu Manager & Hierarchical Submenus (Macintosh Toolbox Essentials ch 3)
 * - Control Manager & Standard CDEFs (Macintosh Toolbox Essentials ch 5)
 * - Dialog Manager & Alerts (Macintosh Toolbox Essentials ch 6)
 * - Palette Manager activation, indexed drawing & animation
 *   (Inside Macintosh Volume VI ch 20)
 * - QuickDraw Geometry, Arcs, Polygons, Regions, PICT, Icons & 3D Bevels
 *   (Imaging With QuickDraw ch 3, 4, 7, 8)
 */

#include <Controls.h>
#include <ControlDefinitions.h>
#include <Dialogs.h>
#include <Events.h>
#include <Fonts.h>
#include <Memory.h>
#include <Menus.h>
#include <OSUtils.h>
#include <Palettes.h>
#include <QDOffscreen.h>
#include <Quickdraw.h>
#include <Resources.h>
#include <ToolUtils.h>
#include <Windows.h>

#if defined(__POWERPC__) || defined(powerc) || defined(PowerPC) || defined(__powerc)
#define SHOWCASE_TARGET_PPC 1
#endif

#ifdef SHOWCASE_TARGET_PPC
#include <QD3D.h>
#include <QD3DCamera.h>
#include <QD3DDrawContext.h>
#include <QD3DGeometry.h>
#include <QD3DGroup.h>
#include <QD3DLight.h>
#include <QD3DRenderer.h>
#include <QD3DShader.h>
#include <QD3DStyle.h>
#include <QD3DTransform.h>
#include <QD3DSet.h>
#endif

#define rMenuBar 128
#define rMainWindow 128
#define rPrefDialog 129
#define rAboutAlert 130
#define rShowcaseIcon 128
#define rShowcasePalette 150

#define mApple 128
#define mPages 129
#define mState 130
#define mFile 131
#define mOptions 132

#define mDifficulty 140
#define mSoundMenu 141
#define mRendererMenu 142

/* Pages menu items */
#define iGraphics 1
#define iControls 2
#define iWindows 3
#define iDrawing 4
#define iPreferences 5
#define iDialogs 6
#define iPalettes 7

/* State menu items */
#define iButtonState 1
#define iCheckboxState 2
#define iScrollbarState 3
#define iWindowState 4

/* Options menu items */
#define iOptDifficulty 1
#define iOptSound 2
#define iOptRenderer 3
#define iOptResetPrefs 5
#define iOptLaunchDialog 6

/* Difficulty submenu items */
#define iDiffEasy 1
#define iDiffNormal 2
#define iDiffHard 3

/* Sound submenu items */
#define iSndMute 1
#define iSndFXOnly 2
#define iSndMusicOnly 3
#define iSndFull 4

/* Renderer submenu items */
#define iRendFlat 1
#define iRendBevel 2
#define iRendContrast 3

/* File menu items */
#define iFilePrefs 1
#define iQuit 4

/* Apple menu items */
#define iAbout 1

/* Page indices */
#define pageGraphics 1
#define pageControls 2
#define pageWindows 3
#define pageDrawing 4
#define pagePreferences 5
#define pageDialogs 6
#define pagePalettes 7

static QDGlobals qd;
static WindowPtr gMainWindow;
static WindowPtr gAuxWindow;

/* Page 2: Controls */
static ControlHandle gButton;
static ControlHandle gCheckbox;
static ControlHandle gScrollbar;

/* Page 5: Game Preferences Controls */
static ControlHandle gPrefSndFX;
static ControlHandle gPrefMusic;
static ControlHandle gPrefVolume;
static ControlHandle gPrefDiffEasy;
static ControlHandle gPrefDiffNormal;
static ControlHandle gPrefDiffHard;
static ControlHandle gPrefRendFlat;
static ControlHandle gPrefRendBevel;
static ControlHandle gPrefRendContrast;
static ControlHandle gPrefBtnApply;
static ControlHandle gPrefBtnReset;
static ControlHandle gPrefBtnModal;

/* Page 6: Dialogs Controls */
static ControlHandle gDlgBtnOpenPrefs;
static ControlHandle gDlgBtnOpenAlert;

/* Page 7: Palette Manager */
static ControlHandle gPaletteAnimate;
static PaletteHandle gShowcasePalette;
static PaletteHandle gOriginalPalette;
static Boolean gPaletteAnimated = false;

/*
 * Record an indexed PixMap into a PICT, replay it through a canonical
 * offscreen GWorld, then copy those pixels into the active screen palette.
 * The picture's 64 populated colors deliberately do not share device indexes
 * with the destination. DrawPicture must color-match them instead of treating
 * the PICT's raw indexes as screen indexes. Imaging With QuickDraw (1994),
 * pp. 4-13 and 7-14; CopyBits palette translation, pp. 7-24..7-28.
 */
static void DrawIndexedPictureTransfer(void)
{
    GWorldPtr sourceWorld;
    GWorldPtr replayWorld;
    GWorldPtr savedWorld;
    GDHandle savedDevice;
    PixMapHandle sourcePixels;
    PixMapHandle replayPixels;
    PixMapHandle windowPixels;
    CTabHandle sourceColors;
    PicHandle picture;
    Rect localRect;
    Rect displayRect;
    Ptr base;
    long rowBytes;
    short x;
    short y;
    short i;
#ifdef SHOWCASE_TARGET_PPC
    RGBColor fallbackColor;
    Rect fallbackBand;

    /* The PPC fixture adapter does not record CopyBits inside OpenPicture. */
    for (i = 0; i < 6; i++) {
        fallbackColor.red = (unsigned short)(0x2200 + i * 0x2400);
        fallbackColor.green = (unsigned short)(0xee00 - i * 0x2200);
        fallbackColor.blue = (unsigned short)(0x3300 + i * 0x1600);
        RGBForeColor(&fallbackColor);
        SetRect(&fallbackBand, 330 + i * 32, 272, 362 + i * 32, 290);
        PaintRect(&fallbackBand);
    }
    fallbackColor.red = fallbackColor.green = fallbackColor.blue = 0;
    RGBForeColor(&fallbackColor);
    SetRect(&fallbackBand, 330, 272, 522, 290);
    FrameRect(&fallbackBand);
    return;
#endif

    sourceWorld = nil;
    replayWorld = nil;
    picture = nil;
    sourceColors = nil;
    SetRect(&localRect, 0, 0, 192, 18);

    GetGWorld(&savedWorld, &savedDevice);
    if (NewGWorld(&sourceWorld, 8, &localRect, nil, nil, 0) != noErr) goto cleanup;
    if (NewGWorld(&replayWorld, 8, &localRect, nil, nil, 0) != noErr) goto cleanup;

    sourcePixels = GetGWorldPixMap(sourceWorld);
    replayPixels = GetGWorldPixMap(replayWorld);
    windowPixels = GetGWorldPixMap((GWorldPtr)gMainWindow);
    if (windowPixels == nil) goto cleanup;
    sourceColors = (**sourcePixels).pmTable;
    if (sourceColors == nil) goto cleanup;
    for (i = 0; i < 64; i++) {
        (**sourceColors).ctTable[i].value = i;
        (**sourceColors).ctTable[i].rgb.red = (unsigned short)(0x2200 + i * 0x02c0);
        (**sourceColors).ctTable[i].rgb.green = (unsigned short)(0xee00 - i * 0x0240);
        (**sourceColors).ctTable[i].rgb.blue = (unsigned short)(0x3300 + i * 0x0180);
    }
    CTabChanged(sourceColors);
    if (!LockPixels(sourcePixels) || !LockPixels(replayPixels)) goto cleanup;

    base = GetPixBaseAddr(sourcePixels);
    rowBytes = (**sourcePixels).rowBytes & 0x3fff;
    for (y = 0; y < 18; y++) {
        for (x = 0; x < 192; x++) {
            base[y * rowBytes + x] = (char)(1 + (x / 3));
        }
    }

    SetGWorld(replayWorld, nil);
    picture = OpenPicture(&localRect);
    CopyBits((BitMap *)*sourcePixels, (BitMap *)*replayPixels,
             &localRect, &localRect, srcCopy, nil);
    ClosePicture();

    DrawPicture(picture, &localRect);

    SetGWorld((GWorldPtr)gMainWindow, savedDevice);
    SetRect(&displayRect, 330, 272, 522, 290);
    CopyBits((BitMap *)*replayPixels, (BitMap *)*windowPixels,
             &localRect, &displayRect, srcCopy, nil);
    FrameRect(&displayRect);

cleanup:
    SetGWorld(savedWorld, savedDevice);
    if (picture != nil) KillPicture(picture);
    if (sourceWorld != nil) DisposeGWorld(sourceWorld);
    if (replayWorld != nil) DisposeGWorld(replayWorld);
}

/*
 * Copy already-authored indexes from an offscreen GDevice whose ColorTable
 * identifies the destination device, but whose RGB entries have been
 * transiently cleared without changing the table seed. Device ColorTables
 * assign colors by position, so CopyBits must preserve these indexes instead
 * of matching their temporary black RGB values into the screen table. Inside
 * Macintosh Volume V (1986), pp. V-57..V-58 and V-138..V-145; Imaging With
 * QuickDraw (1994), pp. 6-16..6-21 and 7-24..7-28.
 */
static void DrawSameDeviceIndexedTransfer(void)
{
    GWorldPtr sourceWorld;
    GDHandle screenDevice;
    PixMapHandle sourcePixels;
    PixMapHandle windowPixels;
    CTabHandle screenColors;
    CTabHandle sourceColors;
    Rect localRect;
    Rect displayRect;
    Ptr base;
    long rowBytes;
    RGBColor color;
    short x;
    short y;

    sourceWorld = nil;
    screenDevice = GetGDevice();
    if (screenDevice == nil) return;
    if ((**(**screenDevice).gdPMap).pixelSize != 8) {
        for (x = 0; x < 3; x++) {
            if (x == 0) {
                color.red = 0xffff; color.green = 0xffff; color.blue = 0;
            } else if (x == 1) {
                color.red = 0xffff; color.green = 0x6666; color.blue = 0;
            } else {
                color.red = 0; color.green = 0xcccc; color.blue = 0xaaaa;
            }
            RGBForeColor(&color);
            SetRect(&displayRect, 330 + x * 64, 302,
                    394 + x * 64, 320);
            PaintRect(&displayRect);
        }
        color.red = color.green = color.blue = 0;
        RGBForeColor(&color);
        SetRect(&displayRect, 330, 302, 522, 320);
        FrameRect(&displayRect);
        return;
    }
    screenColors = (**(**screenDevice).gdPMap).pmTable;
    if (screenColors == nil) return;

    SetRect(&localRect, 0, 0, 192, 18);
    if (NewGWorld(&sourceWorld, 8, &localRect, nil, nil, 0) != noErr) return;

    sourcePixels = GetGWorldPixMap(sourceWorld);
    windowPixels = GetGWorldPixMap((GWorldPtr)gMainWindow);
    if (sourcePixels == nil || windowPixels == nil) goto cleanup;
    sourceColors = (**sourcePixels).pmTable;
    if (sourceColors == nil || !LockPixels(sourcePixels)) goto cleanup;

    base = GetPixBaseAddr(sourcePixels);
    rowBytes = (**sourcePixels).rowBytes & 0x3fff;
    for (y = 0; y < 18; y++) {
        for (x = 0; x < 192; x++) {
            base[y * rowBytes + x] = (char)(2 + (x / 64));
        }
    }

    (**sourceColors).ctSeed = (**screenColors).ctSeed;
    (**sourceColors).ctFlags |= 0x8000;
    for (x = 2; x <= 4; x++) {
        (**sourceColors).ctTable[x].rgb.red = 0;
        (**sourceColors).ctTable[x].rgb.green = 0;
        (**sourceColors).ctTable[x].rgb.blue = 0;
    }

    SetRect(&displayRect, 330, 302, 522, 320);
    CopyBits((BitMap *)*sourcePixels, (BitMap *)*windowPixels,
             &localRect, &displayRect, srcCopy, nil);
    FrameRect(&displayRect);

cleanup:
    if (sourceWorld != nil) DisposeGWorld(sourceWorld);
}

/* State variables */
static short gPage = pageGraphics;
static Boolean gQuit = false;
static Boolean gButtonActivated = false;

/* Preferences state */
static short gDifficulty = iDiffNormal;
static Boolean gSoundFX = true;
static Boolean gMusic = true;
static short gVolume = 75;
static short gRenderer = iRendBevel;
static Boolean gModalDialogCompleted = false;

static MenuHandle StateMenu(void)
{
    return GetMenuHandle(mState);
}

static void DrawHeading(ConstStr255Param heading)
{
    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(24, 34);
    DrawString(heading);
    TextFace(0);
}

/*
 * QuickDraw 3D-style Beveled Treatment (Emulated QuickDraw)
 * Renders raised 3D panels and sunken wells using dual-tone light/shadow borders.
 */
static void DrawBeveledBox(const Rect *r, Boolean sunken)
{
    RGBColor white;
    RGBColor lightGray;
    RGBColor midGray;
    RGBColor darkGray;
    RGBColor black;
    Rect inner;

    white.red = 0xffff; white.green = 0xffff; white.blue = 0xffff;
    lightGray.red = 0xeeee; lightGray.green = 0xeeee; lightGray.blue = 0xeeee;
    midGray.red = 0xcccc; midGray.green = 0xcccc; midGray.blue = 0xcccc;
    darkGray.red = 0x6666; darkGray.green = 0x6666; darkGray.blue = 0x6666;
    black.red = 0x0000; black.green = 0x0000; black.blue = 0x0000;

    RGBForeColor(&black);
    FrameRect(r);

    inner = *r;
    InsetRect(&inner, 1, 1);

    if (!sunken) {
        /* Raised 3D Panel: White top/left, Dark bottom/right */
        RGBForeColor(&white);
        MoveTo(inner.left, inner.bottom - 1);
        LineTo(inner.left, inner.top);
        LineTo(inner.right - 1, inner.top);

        RGBForeColor(&darkGray);
        MoveTo(inner.right - 1, inner.top + 1);
        LineTo(inner.right - 1, inner.bottom - 1);
        LineTo(inner.left + 1, inner.bottom - 1);

        InsetRect(&inner, 1, 1);
        RGBForeColor(&lightGray);
        PaintRect(&inner);
    } else {
        /* Sunken 3D Well: Dark top/left, White bottom/right */
        RGBForeColor(&darkGray);
        MoveTo(inner.left, inner.bottom - 1);
        LineTo(inner.left, inner.top);
        LineTo(inner.right - 1, inner.top);

        RGBForeColor(&white);
        MoveTo(inner.right - 1, inner.top + 1);
        LineTo(inner.right - 1, inner.bottom - 1);
        LineTo(inner.left + 1, inner.bottom - 1);

        InsetRect(&inner, 1, 1);
        RGBForeColor(&midGray);
        PaintRect(&inner);
    }
    RGBForeColor(&black);
}

static void DrawGraphicsPage(void)
{
    Rect r;
    Rect clip;
    RgnHandle savedClip;
    RGBColor red;
    RGBColor blue;
    RGBColor black;

    DrawHeading("\pGraphics, patterns, clipping, color, and text");

    SetRect(&r, 24, 55, 174, 135);
    PenPat(&qd.gray);
    PaintRect(&r);
    PenNormal();
    FrameRect(&r);

    red.red = 0xffff;
    red.green = 0x2222;
    red.blue = 0x2222;
    blue.red = 0x2222;
    blue.green = 0x4444;
    blue.blue = 0xffff;
    black.red = black.green = black.blue = 0;

    RGBForeColor(&red);
    SetRect(&r, 205, 55, 325, 135);
    PaintOval(&r);

    savedClip = NewRgn();
    GetClip(savedClip);
    SetRect(&clip, 405, 35, 486, 156);
    ClipRect(&clip);
    RGBForeColor(&blue);
    SetRect(&r, 330, 35, 485, 155);
    FrameRoundRect(&r, 24, 24);
    SetClip(savedClip);
    DisposeRgn(savedClip);

    RGBForeColor(&black);
    MoveTo(24, 175);
    LineTo(505, 175);
    TextFont(3);
    TextSize(10);
    MoveTo(24, 205);
    DrawString("\pThe same source and resources drive both CPU slices.");
}

static void DrawControlsPage(void)
{
    DrawHeading("\pControls and scroll bars");
    MoveTo(24, 70);
    DrawString("\pClick each control; the State menu records the result.");
    DrawControls(gMainWindow);
}

static void DrawWindowsPage(void)
{
    DrawHeading("\pWindow lifecycle and update events");
    MoveTo(24, 70);
    DrawString("\pThis page opens a second document window.");
    MoveTo(24, 92);
    DrawString("\pChoose another page to dispose it again.");
}

#ifdef SHOWCASE_TARGET_PPC
/*
 * Native PowerPC QuickDraw 3D Rendering Pipeline.
 * Exercises real QuickDraw3DLib lifecycle:
 * - Initialization & Teardown (Q3Initialize, Q3Exit)
 * - Macintosh Draw Context with Pane Viewport (Q3MacDrawContext_New)
 * - View Angle Aspect Perspective Camera (Q3ViewAngleAspectCamera_New)
 * - Interactive Renderer (Q3Renderer_NewFromType)
 * - Multi-source Lighting: Ambient & Directional (Q3LightGroup_New, Q3AmbientLight_New, Q3DirectionalLight_New)
 * - Material Attribute Set with Diffuse Color (Q3AttributeSet_New, Q3AttributeSet_Add)
 * - 3D Geometry: TriMesh model with vertex points & triangle indexing (Q3TriMesh_New)
 * - Phong Illumination Shader (Q3PhongIllumination_New)
 * - Rendering traversal loop (Q3View_StartRendering, Q3InterpolationStyle_Submit, Q3BackfacingStyle_Submit, Q3FillStyle_Submit, Q3Shader_Submit, Q3TriMesh_Submit, Q3View_EndRendering)
 * - Comprehensive resource disposal (Q3Object_Dispose)
 */
static void RenderQD3DScene(WindowPtr window)
{
    TQ3DrawContextObject drawContext = nil;
    TQ3CameraObject camera = nil;
    TQ3RendererObject renderer = nil;
    TQ3GroupObject lightGroup = nil;
    TQ3LightObject ambientLight = nil;
    TQ3LightObject dirLight = nil;
    TQ3ViewObject view = nil;
    TQ3ShaderObject shader = nil;
    TQ3AttributeSet attrSet = nil;
    TQ3GeometryObject geom = nil;
    TQ3MacDrawContextData macDCData;
    TQ3ViewAngleAspectCameraData cameraData;
    TQ3LightData ambData;
    TQ3DirectionalLightData dirData;
    TQ3ColorRGB diffColor;
    TQ3TriMeshData triMeshData;
    TQ3Point3D points[4];
    TQ3TriMeshTriangleData triangles[4];
    TQ3ViewStatus viewStatus;

    if (Q3Initialize() != kQ3Success) {
        return;
    }

    /* Bounded 3D Viewport Pane within Section 1 of Drawing Page: local (30, 80, 125, 130) */
    macDCData.drawContextData.clearImageMethod = kQ3ClearMethodWithColor;
    macDCData.drawContextData.clearImageColor.a = 1.0f;
    macDCData.drawContextData.clearImageColor.r = 0.12f;
    macDCData.drawContextData.clearImageColor.g = 0.16f;
    macDCData.drawContextData.clearImageColor.b = 0.28f;
    macDCData.drawContextData.pane.min.x = 30.0f;
    macDCData.drawContextData.pane.min.y = 80.0f;
    macDCData.drawContextData.pane.max.x = 125.0f;
    macDCData.drawContextData.pane.max.y = 130.0f;
    macDCData.drawContextData.paneState = kQ3True;
    macDCData.drawContextData.mask.image = nil;
    macDCData.drawContextData.mask.width = 0;
    macDCData.drawContextData.mask.height = 0;
    macDCData.drawContextData.mask.rowBytes = 0;
    macDCData.drawContextData.mask.bitOrder = kQ3EndianBig;
    macDCData.drawContextData.maskState = kQ3False;
    macDCData.drawContextData.doubleBufferState = kQ3False;
    macDCData.window = (CWindowPtr)window;
    macDCData.library = kQ3Mac2DLibraryNone;
    macDCData.viewPort = nil;
    macDCData.grafPort = (CGrafPtr)window;

    drawContext = Q3MacDrawContext_New(&macDCData);

    cameraData.cameraData.placement.cameraLocation.x = 0.0f;
    cameraData.cameraData.placement.cameraLocation.y = 0.35f;
    cameraData.cameraData.placement.cameraLocation.z = 2.4f;
    cameraData.cameraData.placement.pointOfInterest.x = 0.0f;
    cameraData.cameraData.placement.pointOfInterest.y = 0.0f;
    cameraData.cameraData.placement.pointOfInterest.z = 0.0f;
    cameraData.cameraData.placement.upVector.x = 0.0f;
    cameraData.cameraData.placement.upVector.y = 1.0f;
    cameraData.cameraData.placement.upVector.z = 0.0f;
    cameraData.cameraData.range.hither = 0.1f;
    cameraData.cameraData.range.yon = 100.0f;
    cameraData.cameraData.viewPort.origin.x = -1.0f;
    cameraData.cameraData.viewPort.origin.y = 1.0f;
    cameraData.cameraData.viewPort.width = 2.0f;
    cameraData.cameraData.viewPort.height = 2.0f;
    cameraData.fov = 0.85f;
    cameraData.aspectRatioXToY = 1.0f;

    camera = Q3ViewAngleAspectCamera_New(&cameraData);

    renderer = Q3Renderer_NewFromType(kQ3RendererTypeInteractive);

    lightGroup = Q3LightGroup_New();
    if (lightGroup != nil) {
        ambData.isOn = kQ3True;
        ambData.brightness = 0.35f;
        ambData.color.r = 1.0f;
        ambData.color.g = 1.0f;
        ambData.color.b = 1.0f;
        ambientLight = Q3AmbientLight_New(&ambData);
        if (ambientLight != nil) {
            Q3Group_AddObject(lightGroup, ambientLight);
            Q3Object_Dispose(ambientLight);
        }

        dirData.lightData.isOn = kQ3True;
        dirData.lightData.brightness = 0.85f;
        dirData.lightData.color.r = 1.0f;
        dirData.lightData.color.g = 1.0f;
        dirData.lightData.color.b = 1.0f;
        dirData.castsShadows = kQ3False;
        dirData.direction.x = 1.0f;
        dirData.direction.y = -1.0f;
        dirData.direction.z = -1.0f;
        dirLight = Q3DirectionalLight_New(&dirData);
        if (dirLight != nil) {
            Q3Group_AddObject(lightGroup, dirLight);
            Q3Object_Dispose(dirLight);
        }
    }

    view = Q3View_New();
    if (view != nil) {
        if (drawContext != nil) Q3View_SetDrawContext(view, drawContext);
        if (camera != nil) Q3View_SetCamera(view, camera);
        if (renderer != nil) Q3View_SetRenderer(view, renderer);
        if (lightGroup != nil) Q3View_SetLightGroup(view, lightGroup);
    }

    /* 3D Tetrahedron / Pyramid Model */
    points[0].x =  0.0f; points[0].y =  0.55f; points[0].z =  0.0f;
    points[1].x = -0.5f; points[1].y = -0.35f; points[1].z =  0.5f;
    points[2].x =  0.5f; points[2].y = -0.35f; points[2].z =  0.5f;
    points[3].x =  0.0f; points[3].y = -0.35f; points[3].z = -0.5f;

    triangles[0].pointIndices[0] = 0;
    triangles[0].pointIndices[1] = 1;
    triangles[0].pointIndices[2] = 2;

    triangles[1].pointIndices[0] = 0;
    triangles[1].pointIndices[1] = 2;
    triangles[1].pointIndices[2] = 3;

    triangles[2].pointIndices[0] = 0;
    triangles[2].pointIndices[1] = 3;
    triangles[2].pointIndices[2] = 1;

    triangles[3].pointIndices[0] = 1;
    triangles[3].pointIndices[1] = 3;
    triangles[3].pointIndices[2] = 2;

    attrSet = Q3AttributeSet_New();
    if (attrSet != nil) {
        diffColor.r = 0.2f;
        diffColor.g = 0.65f;
        diffColor.b = 0.95f;
        Q3AttributeSet_Add(attrSet, kQ3AttributeTypeDiffuseColor, &diffColor);
    }

    triMeshData.triMeshAttributeSet = attrSet;
    triMeshData.numTriangles = 4;
    triMeshData.triangles = triangles;
    triMeshData.numTriangleAttributeTypes = 0;
    triMeshData.triangleAttributeTypes = nil;
    triMeshData.numEdges = 0;
    triMeshData.edges = nil;
    triMeshData.numEdgeAttributeTypes = 0;
    triMeshData.edgeAttributeTypes = nil;
    triMeshData.numPoints = 4;
    triMeshData.points = points;
    triMeshData.numVertexAttributeTypes = 0;
    triMeshData.vertexAttributeTypes = nil;
    triMeshData.bBox.isEmpty = kQ3False;
    triMeshData.bBox.min.x = -0.5f;
    triMeshData.bBox.min.y = -0.35f;
    triMeshData.bBox.min.z = -0.5f;
    triMeshData.bBox.max.x =  0.5f;
    triMeshData.bBox.max.y =  0.55f;
    triMeshData.bBox.max.z =  0.5f;

    geom = Q3TriMesh_New(&triMeshData);
    shader = Q3PhongIllumination_New();

    if (view != nil && Q3View_StartRendering(view) == kQ3Success) {
        do {
            Q3InterpolationStyle_Submit(kQ3InterpolationStyleVertex, view);
            Q3BackfacingStyle_Submit(kQ3BackfacingStyleBoth, view);
            Q3FillStyle_Submit(kQ3FillStyleFilled, view);
            if (shader != nil) {
                Q3Shader_Submit(shader, view);
            }
            Q3TriMesh_Submit(&triMeshData, view);
            viewStatus = Q3View_EndRendering(view);
        } while (viewStatus == kQ3ViewStatusRetraverse);
    }

    if (geom != nil) Q3Object_Dispose(geom);
    if (shader != nil) Q3Object_Dispose(shader);
    if (attrSet != nil) Q3Object_Dispose(attrSet);
    if (view != nil) Q3Object_Dispose(view);
    if (lightGroup != nil) Q3Object_Dispose(lightGroup);
    if (renderer != nil) Q3Object_Dispose(renderer);
    if (camera != nil) Q3Object_Dispose(camera);
    if (drawContext != nil) Q3Object_Dispose(drawContext);

    Q3Exit();
}
#endif

/*
 * Page 4: Broader Drawing, QuickDraw 3D, Polygons, Arcs, Regions, PICT & Text
 */
static void DrawDrawingPage(void)
{
    Rect r;
    Rect arcRect;
    Rect subRect;
    Rect picFrame;
    Rect dstRect;
    PolyHandle poly;
    RgnHandle rgnA;
    RgnHandle rgnB;
    RgnHandle rgnCombined;
    PicHandle pic;
    Handle iconH;
    RGBColor color;
    RGBColor white;
    RGBColor darkGray;
    RGBColor black;
    FontInfo fInfo;
    short strW;
    Str255 strBuf;

    white.red = white.green = white.blue = 0xffff;
    darkGray.red = darkGray.green = darkGray.blue = 0x5555;
    black.red = black.green = black.blue = 0x0000;

    /* Section 1: QuickDraw 3D Native PowerPC vs 68K Beveled Treatment */
    SetRect(&r, 20, 48, 270, 165);
    DrawBeveledBox(&r, false);

#ifdef SHOWCASE_TARGET_PPC
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pNative PowerPC QuickDraw 3D");
    TextFace(0);
    MoveTo(28, 74);
    DrawString("\p(Interactive 3D Pipeline & TriMesh)");

    /* Sunken 3D Viewport Frame */
    SetRect(&subRect, 29, 79, 126, 131);
    DrawBeveledBox(&subRect, true);

    /* Real QuickDraw 3D Render Pass into Bounded Viewport Pane */
    RenderQD3DScene(gMainWindow);

    /* Sunken 3D Gauge Well beside 3D viewport */
    SetRect(&subRect, 135, 95, 260, 125);
    DrawBeveledBox(&subRect, true);
    color.red = 0x2222; color.green = 0x8888; color.blue = 0x3333;
    RGBForeColor(&color);
    SetRect(&arcRect, 138, 98, 215, 122);
    PaintRect(&arcRect);
    RGBForeColor(&black);
    TextFont(applFont);
    TextSize(9);
    MoveTo(145, 113);
    DrawString("\pGauge: 65%");

    /* Inset status bar */
    SetRect(&subRect, 30, 136, 260, 157);
    DrawBeveledBox(&subRect, true);
    TextFont(3);
    TextSize(9);
    MoveTo(38, 150);
    DrawString("\pQ3 View / Camera / Lights / TriMesh");

#else
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pQuickDraw 3D-style Beveled Treatment");
    TextFace(0);
    MoveTo(28, 74);
    DrawString("\p(68K 2D Fallback Representation)");

    /* Metallic horizontal ridges */
    RGBForeColor(&white);
    MoveTo(28, 80); LineTo(262, 80);
    MoveTo(28, 84); LineTo(262, 84);
    RGBForeColor(&darkGray);
    MoveTo(28, 81); LineTo(262, 81);
    MoveTo(28, 85); LineTo(262, 85);

    /* Raised 3D Button */
    SetRect(&subRect, 30, 95, 125, 125);
    DrawBeveledBox(&subRect, false);
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(44, 113);
    DrawString("\pEmbossed");
    TextFace(0);

    /* Sunken 3D Gauge Well */
    SetRect(&subRect, 135, 95, 260, 125);
    DrawBeveledBox(&subRect, true);
    color.red = 0x2222; color.green = 0x8888; color.blue = 0x3333;
    RGBForeColor(&color);
    SetRect(&arcRect, 138, 98, 215, 122);
    PaintRect(&arcRect);
    RGBForeColor(&black);
    TextFont(applFont);
    TextSize(9);
    MoveTo(145, 113);
    DrawString("\pGauge: 65%");

    /* Inset chamfer status bar */
    SetRect(&subRect, 30, 134, 260, 155);
    DrawBeveledBox(&subRect, true);
    TextFont(3);
    TextSize(9);
    MoveTo(38, 148);
    DrawString("\pBeveled Facet Lighting (White / Gray)");
#endif

    /* Section 2: Polygons & Arcs */
    SetRect(&r, 280, 48, 535, 165);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(288, 62);
    DrawString("\pPolygons and Arcs (QuickDraw Geometry)");
    TextFace(0);

    /* Star Polygon */
    poly = OpenPoly();
    MoveTo(345, 75);
    LineTo(356, 102); LineTo(385, 102); LineTo(362, 118);
    LineTo(371, 148); LineTo(345, 130); LineTo(319, 148);
    LineTo(328, 118); LineTo(305, 102); LineTo(334, 102);
    LineTo(345, 75);
    ClosePoly();

    color.red = 0xeeee; color.green = 0x9999; color.blue = 0x1111;
    RGBForeColor(&color);
    PaintPoly(poly);
    RGBForeColor(&black);
    FramePoly(poly);
    KillPoly(poly);

    /* 3-Sector Pie Chart with Arcs */
    SetRect(&arcRect, 415, 78, 495, 158);
    color.red = 0xdddd; color.green = 0x2222; color.blue = 0x3333;
    RGBForeColor(&color);
    PaintArc(&arcRect, 0, 120);

    color.red = 0x2222; color.green = 0x9999; color.blue = 0x4444;
    RGBForeColor(&color);
    PaintArc(&arcRect, 120, 110);

    color.red = 0x2222; color.green = 0x4444; color.blue = 0xdddd;
    RGBForeColor(&color);
    PaintArc(&arcRect, 230, 130);

    RGBForeColor(&black);
    FrameOval(&arcRect);

    /* Section 3: QuickDraw Regions */
    SetRect(&r, 20, 175, 185, 320);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 190);
    DrawString("\pQuickDraw Regions");
    TextFace(0);

    rgnA = NewRgn();
    rgnB = NewRgn();
    rgnCombined = NewRgn();

    SetRect(&subRect, 30, 205, 105, 275);
    OpenRgn();
    FrameOval(&subRect);
    CloseRgn(rgnA);

    SetRect(&subRect, 65, 225, 140, 290);
    OpenRgn();
    FrameRoundRect(&subRect, 16, 16);
    CloseRgn(rgnB);

    XorRgn(rgnA, rgnB, rgnCombined);
    color.red = 0x3333; color.green = 0x6666; color.blue = 0xbbbb;
    RGBForeColor(&color);
    PaintRgn(rgnCombined);
    RGBForeColor(&black);
    FrameRgn(rgnCombined);

    DisposeRgn(rgnA);
    DisposeRgn(rgnB);
    DisposeRgn(rgnCombined);

    TextFont(3);
    TextSize(9);
    MoveTo(28, 308);
    DrawString("\pXorRgn Oval + RoundRect");

    /* Section 4: Icons & Picture (PICT) Recording */
    SetRect(&r, 195, 175, 360, 320);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(203, 190);
    DrawString("\pIcons & Pictures (PICT)");
    TextFace(0);

    /* Plot standard icon resource */
    SetRect(&subRect, 205, 205, 237, 237);
    iconH = GetIcon(rShowcaseIcon);
    if (iconH != nil) {
        PlotIcon(&subRect, iconH);
        ReleaseResource(iconH);
    }

    /* Dynamic PICT Recording & Playback */
    SetRect(&picFrame, 0, 0, 40, 40);
    pic = OpenPicture(&picFrame);
    color.red = 0xeeee; color.green = 0x7777; color.blue = 0x1111;
    RGBForeColor(&color);
#ifdef SHOWCASE_TARGET_PPC
    PaintRect(&picFrame);
#else
    PaintRoundRect(&picFrame, 10, 10);
#endif
    RGBForeColor(&black);
    FrameRoundRect(&picFrame, 10, 10);
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(6, 25);
    DrawString("\pPICT");
    TextFace(0);
    ClosePicture();

    /* Draw picture 1:1 and scaled */
    SetRect(&dstRect, 250, 205, 290, 245);
    DrawPicture(pic, &dstRect);
    SetRect(&dstRect, 300, 205, 350, 255);
    DrawPicture(pic, &dstRect);
    KillPicture(pic);

    TextFont(3);
    TextSize(9);
    MoveTo(203, 280);
    DrawString("\pPlotIcon (32x32 bitmap)");
    MoveTo(203, 295);
    DrawString("\pOpenPicture/DrawPicture");

    /* Section 5: Typography & Measurements */
    SetRect(&r, 370, 175, 535, 320);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(378, 190);
    DrawString("\pTypography & Metrics");
    TextFace(0);

    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(378, 212);
    DrawString("\pSystem 12pt Bold");

    TextFont(applFont);
    TextSize(9);
    TextFace(0);
    MoveTo(378, 228);
    DrawString("\pGeneva 9pt Plain Text");

    TextFont(4); /* Monaco */
    TextSize(9);
    MoveTo(378, 244);
    DrawString("\pMonaco 9pt Code Font");

    TextFont(applFont);
    TextSize(9);
    TextFace(italic | underline | shadow);
    MoveTo(378, 262);
    DrawString("\pItalic Underline Shadow");
    TextFace(0);

    /* Text measurement */
    GetFontInfo(&fInfo);
    strBuf[0] = 0;
    MoveTo(378, 282);
    TextFont(3);
    TextSize(9);
    DrawString("\pMeasureText: ");
    strW = StringWidth("\pToolbox Showcase");
    NumToString(strW, strBuf);
    DrawString(strBuf);
    DrawString("\ppx");

    /* Ruler line */
    MoveTo(378, 295);
    LineTo(378 + (strW > 140 ? 140 : strW), 295);

    /* Footer note */
    MoveTo(20, 345);
    TextFont(applFont);
    TextSize(9);
    DrawString("\pAll operations executed through standard QuickDraw traps on 68K and PowerPC.");

    /* Draw this last because QuickDraw 3D flushes its pane asynchronously. */
    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(70, 34);
    DrawString("\pDrawing: QuickDraw 3D, Polygons, Arcs, Regions, Pictures & Text");
    TextFace(0);
}

/*
 * Page 5: Game Preferences & Settings Panel
 */
static void DrawPreferencesPage(void)
{
    Rect r;
    Rect well;
    Str255 volStr;

    DrawHeading("\pGame Preferences & Configuration Panel");

    /* Audio Settings Group Box */
    SetRect(&r, 20, 48, 230, 160);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 62);
    DrawString("\pAudio & Sound FX");
    TextFace(0);

    /* Difficulty Group Box */
    SetRect(&r, 240, 48, 535, 160);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(248, 62);
    DrawString("\pGameplay Difficulty");
    TextFace(0);

    /* Volume Slider Box */
    SetRect(&r, 20, 170, 230, 255);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 184);
    DrawString("\pMaster Volume: ");
    NumToString(gVolume, volStr);
    DrawString(volStr);
    DrawString("\p%");
    TextFace(0);

    /* Renderer Group Box */
    SetRect(&r, 240, 170, 535, 255);
    DrawBeveledBox(&r, false);
    TextFont(systemFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(248, 184);
    DrawString("\pGraphics Pipeline / Shading Style");
    TextFace(0);

    /* Status Readout Sunken Well */
    SetRect(&well, 20, 265, 535, 305);
    DrawBeveledBox(&well, true);
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(28, 282);
    DrawString("\pActive Profile: ");
    TextFace(0);
    if (gDifficulty == iDiffEasy) {
        DrawString("\pRecruit (Easy)");
    } else if (gDifficulty == iDiffHard) {
        DrawString("\pNightmare (Hard)");
    } else {
        DrawString("\pVeteran (Normal)");
    }
    DrawString("\p | Audio: ");
    if (!gSoundFX && !gMusic) {
        DrawString("\pMuted");
    } else if (gSoundFX && !gMusic) {
        DrawString("\pFX Only");
    } else if (!gSoundFX && gMusic) {
        DrawString("\pMusic Only");
    } else {
        DrawString("\pFull (FX+Music)");
    }
    DrawString("\p | Volume: ");
    DrawString(volStr);
    DrawString("\p% | Pipeline: ");
    if (gRenderer == iRendFlat) {
        DrawString("\pFlat 2D");
    } else if (gRenderer == iRendContrast) {
        DrawString("\pHigh Contrast");
    } else {
        DrawString("\pQD3D Bevels");
    }

    MoveTo(28, 297);
    TextFont(3);
    TextSize(9);
    DrawString("\pSettings synchronize bidirectionally with the Options hierarchical menus.");

    DrawControls(gMainWindow);
}

/*
 * Page 6: Dialog Manager, Modal Dialogs & System Alerts
 */
static void DrawDialogsPage(void)
{
    Rect r;
    Rect btnRect;
    Rect editRect;
    RGBColor black;

    black.red = black.green = black.blue = 0x0000;

    DrawHeading("\pDialog Manager: Modal Dialogs & System Alerts");

    TextFont(applFont);
    TextSize(9);
    MoveTo(24, 52);
    DrawString("\pThe Dialog Manager provides standardized modal and modeless user interaction.");
    MoveTo(24, 66);
    DrawString("\pClick the action buttons below or use the Options menu to trigger live sessions.");

    /* Embedded Dialog Simulation Preview */
    SetRect(&r, 20, 80, 535, 290);
    DrawBeveledBox(&r, false);

    TextFont(systemFont);
    TextSize(12);
    TextFace(bold);
    MoveTo(35, 105);
    DrawString("\pSimulated Modal Dialog Structure (DLOG / DITL)");
    TextFace(0);

    /* Default Button 3px Bold Outline Ring */
    SetRect(&btnRect, 410, 235, 510, 265);
    PenSize(3, 3);
    FrameRoundRect(&btnRect, 16, 16);
    PenNormal();
    TextFont(systemFont);
    TextSize(12);
    MoveTo(445, 255);
    DrawString("\pOK");

    /* Cancel Button */
    SetRect(&btnRect, 295, 238, 385, 262);
    FrameRoundRect(&btnRect, 8, 8);
    MoveTo(320, 255);
    DrawString("\pCancel");

    /* Dialog Item Types showcase */
    TextFont(applFont);
    TextSize(9);
    MoveTo(35, 130);
    DrawString("\p* Item Type 1 (btnCtrl): Standard & Default Action Push Buttons (3px ring)");
    MoveTo(35, 150);
    DrawString("\p* Item Type 2 (chkCtrl / radCtrl): Checkboxes & Mutual Exclusion Radio Buttons");
    MoveTo(35, 170);
    DrawString("\p* Item Type 3 (statText): Non-editable Information & Prompt Strings");
    MoveTo(35, 190);
    DrawString("\p* Item Type 4 (editText): TextEdit Buffer with Keyboard Focus & Selection:");

    /* Sample EditText Field */
    SetRect(&editRect, 170, 202, 380, 222);
    RGBForeColor(&black);
    FrameRect(&editRect);
    TextFont(4); /* Monaco */
    TextSize(9);
    MoveTo(176, 216);
    DrawString("\pAce Pilot |");

    /* Status of last modal session */
    TextFont(applFont);
    TextSize(9);
    TextFace(bold);
    MoveTo(35, 282);
    DrawString("\pModal Dialog Status: ");
    TextFace(0);
    if (gModalDialogCompleted) {
        DrawString("\pLast session confirmed with OK.");
    } else {
        DrawString("\pNo modal dialog session completed yet.");
    }

    DrawControls(gMainWindow);
}

/*
 * Palette Manager page. Animated + explicit entries deliberately occupy
 * stable device indexes so AnimateEntry recolors already-drawn pixels without
 * touching the window bitmap. Inside Macintosh Volume VI (1991), pp. 20-10
 * through 20-15 and 20-19 through 20-22.
 */
static void DrawPalettesPage(void)
{
    Rect r;
    RGBColor black;
    RGBColor white;

    black.red = black.green = black.blue = 0;
    white.red = white.green = white.blue = 0xffff;

    RGBForeColor(&black);
    RGBBackColor(&white);
    DrawHeading("\pPalette activation, indexed drawing, and animation");
    MoveTo(24, 58);
    DrawString("\pThe swatches use PmForeColor; the lower well uses PmBackColor.");
    MoveTo(24, 76);
    DrawString("\pAnimated + explicit colors stay at known CLUT indexes.");

    PmForeColor(2);
    SetRect(&r, 30, 96, 180, 166);
    PaintRect(&r);
    PmForeColor(3);
    SetRect(&r, 205, 96, 355, 166);
    PaintOval(&r);
    PmForeColor(4);
    SetRect(&r, 380, 96, 530, 166);
    PaintRect(&r);

    RGBForeColor(&black);
    SetRect(&r, 30, 96, 180, 166); FrameRect(&r);
    SetRect(&r, 205, 96, 355, 166); FrameOval(&r);
    SetRect(&r, 380, 96, 530, 166); FrameRoundRect(&r, 18, 18);
    MoveTo(62, 186); DrawString("\pEntry 2");
    MoveTo(246, 186); DrawString("\pEntry 3");
    MoveTo(422, 186); DrawString("\pEntry 4");

    SetRect(&r, 30, 205, 530, 254);
    FrameRect(&r);
    InsetRect(&r, 2, 2);
    PmBackColor(5);
    EraseRect(&r);
    RGBBackColor(&white);
    RGBForeColor(&black);
    MoveTo(48, 235);
    DrawString("\pTolerant background entry allocated by the Palette Manager");

    RGBBackColor(&white);
    RGBForeColor(&black);
    MoveTo(30, 284);
    DrawString("\pIndexed PICT -> GWorld -> screen");
    MoveTo(30, 314);
    DrawString("\pDevice indexes survive black CLUT");
    DrawSameDeviceIndexedTransfer();
    DrawIndexedPictureTransfer();

    MoveTo(205, 348);
    DrawString(gPaletteAnimated ? "\pAnimated CLUT values" : "\pInitial CLUT values");
    DrawControls(gMainWindow);
}

static void AnimateShowcasePalette(void)
{
    RGBColor color;

    gPaletteAnimated = !gPaletteAnimated;
    color.red = 0xffff;
    color.green = gPaletteAnimated ? 0x1111 : 0xffff;
    color.blue = gPaletteAnimated ? 0x9999 : 0x0000;
    AnimateEntry(gMainWindow, 2, &color);

    color.red = gPaletteAnimated ? 0x1111 : 0xffff;
    color.green = gPaletteAnimated ? 0xdddd : 0x6666;
    color.blue = gPaletteAnimated ? 0x5555 : 0x0000;
    AnimateEntry(gMainWindow, 3, &color);

    color.red = gPaletteAnimated ? 0x2222 : 0x0000;
    color.green = gPaletteAnimated ? 0x5555 : 0xcccc;
    color.blue = gPaletteAnimated ? 0xffff : 0xaaaa;
    AnimateEntry(gMainWindow, 4, &color);
}

static void DrawMainWindow(void)
{
    SetPort(gMainWindow);
    EraseRect(&gMainWindow->portRect);
    switch (gPage) {
        case pageGraphics:
            DrawGraphicsPage();
            break;
        case pageControls:
            DrawControlsPage();
            break;
        case pageWindows:
            DrawWindowsPage();
            break;
        case pageDrawing:
            DrawDrawingPage();
            break;
        case pagePreferences:
            DrawPreferencesPage();
            break;
        case pageDialogs:
            DrawDialogsPage();
            break;
        case pagePalettes:
            DrawPalettesPage();
            break;
    }
}

static void DrawAuxWindow(void)
{
    if (gAuxWindow == nil) {
        return;
    }
    SetPort(gAuxWindow);
    EraseRect(&gAuxWindow->portRect);
    DrawHeading("\pAuxiliary window");
    MoveTo(24, 68);
    DrawString("\pCreated and destroyed through ordinary Window Manager calls.");
}

static void ShowAllControls(short page)
{
    Boolean isControls = (page == pageControls);
    Boolean isPrefs = (page == pagePreferences);
    Boolean isDialogs = (page == pageDialogs);
    Boolean isPalettes = (page == pagePalettes);

    /* Page 2: Controls */
    if (isControls) {
        ShowControl(gButton);
        ShowControl(gCheckbox);
        ShowControl(gScrollbar);
    } else {
        HideControl(gButton);
        HideControl(gCheckbox);
        HideControl(gScrollbar);
    }

    /* Page 5: Game Preferences */
    if (isPrefs) {
        ShowControl(gPrefSndFX);
        ShowControl(gPrefMusic);
        ShowControl(gPrefVolume);
        ShowControl(gPrefDiffEasy);
        ShowControl(gPrefDiffNormal);
        ShowControl(gPrefDiffHard);
        ShowControl(gPrefRendFlat);
        ShowControl(gPrefRendBevel);
        ShowControl(gPrefRendContrast);
        ShowControl(gPrefBtnApply);
        ShowControl(gPrefBtnReset);
        ShowControl(gPrefBtnModal);
    } else {
        HideControl(gPrefSndFX);
        HideControl(gPrefMusic);
        HideControl(gPrefVolume);
        HideControl(gPrefDiffEasy);
        HideControl(gPrefDiffNormal);
        HideControl(gPrefDiffHard);
        HideControl(gPrefRendFlat);
        HideControl(gPrefRendBevel);
        HideControl(gPrefRendContrast);
        HideControl(gPrefBtnApply);
        HideControl(gPrefBtnReset);
        HideControl(gPrefBtnModal);
    }

    /* Page 6: Dialogs */
    if (isDialogs) {
        ShowControl(gDlgBtnOpenPrefs);
        ShowControl(gDlgBtnOpenAlert);
    } else {
        HideControl(gDlgBtnOpenPrefs);
        HideControl(gDlgBtnOpenAlert);
    }

    /* Page 7: Palette Manager */
    if (isPalettes) {
        ShowControl(gPaletteAnimate);
    } else {
        HideControl(gPaletteAnimate);
    }
}

static void SyncMenuState(void)
{
    MenuHandle pages;
    MenuHandle hDiff;
    MenuHandle hSound;
    MenuHandle hRend;
    short i;

    pages = GetMenuHandle(mPages);
    if (pages != nil) {
        for (i = 1; i <= 7; i++) {
            CheckItem(pages, i, gPage == i);
        }
    }

    hDiff = GetMenuHandle(mDifficulty);
    if (hDiff != nil) {
        CheckItem(hDiff, iDiffEasy, gDifficulty == iDiffEasy);
        CheckItem(hDiff, iDiffNormal, gDifficulty == iDiffNormal);
        CheckItem(hDiff, iDiffHard, gDifficulty == iDiffHard);
    }

    hSound = GetMenuHandle(mSoundMenu);
    if (hSound != nil) {
        CheckItem(hSound, iSndMute, !gSoundFX && !gMusic);
        CheckItem(hSound, iSndFXOnly, gSoundFX && !gMusic);
        CheckItem(hSound, iSndMusicOnly, !gSoundFX && gMusic);
        CheckItem(hSound, iSndFull, gSoundFX && gMusic);
    }

    hRend = GetMenuHandle(mRendererMenu);
    if (hRend != nil) {
        CheckItem(hRend, iRendFlat, gRenderer == iRendFlat);
        CheckItem(hRend, iRendBevel, gRenderer == iRendBevel);
        CheckItem(hRend, iRendContrast, gRenderer == iRendContrast);
    }

    /* Update control handles if created */
    if (gPrefSndFX != nil) {
        SetControlValue(gPrefSndFX, gSoundFX ? 1 : 0);
        SetControlValue(gPrefMusic, gMusic ? 1 : 0);
        SetControlValue(gPrefVolume, gVolume);
        SetControlValue(gPrefDiffEasy, gDifficulty == iDiffEasy ? 1 : 0);
        SetControlValue(gPrefDiffNormal, gDifficulty == iDiffNormal ? 1 : 0);
        SetControlValue(gPrefDiffHard, gDifficulty == iDiffHard ? 1 : 0);
        SetControlValue(gPrefRendFlat, gRenderer == iRendFlat ? 1 : 0);
        SetControlValue(gPrefRendBevel, gRenderer == iRendBevel ? 1 : 0);
        SetControlValue(gPrefRendContrast, gRenderer == iRendContrast ? 1 : 0);
    }
}

static void DoModalPrefsDialog(void)
{
    DialogPtr theDialog;
    short itemHit;
    short itemType;
    Handle itemHandle;
    Rect itemRect;

    theDialog = GetNewDialog(rPrefDialog, nil, (WindowPtr)-1);
    if (theDialog == nil) {
        return;
    }
    SetPort(theDialog);
    ShowWindow(theDialog);

    do {
        ModalDialog(nil, &itemHit);
        if (itemHit == 4 || itemHit == 5) {
            GetDialogItem(theDialog, itemHit, &itemType, &itemHandle, &itemRect);
            SetControlValue((ControlHandle)itemHandle,
                            GetControlValue((ControlHandle)itemHandle) == 0 ? 1 : 0);
        }
    } while (itemHit != 1 && itemHit != 2);

    if (itemHit == 1) {
        gModalDialogCompleted = true;
    }

    DisposeDialog(theDialog);
    SetPort(gMainWindow);
    DrawMainWindow();
}

static void DoAboutAlert(void)
{
    Alert(rAboutAlert, nil);
    SetPort(gMainWindow);
    DrawMainWindow();
}

static void SetPage(short page)
{
    Rect bounds;

    if (gPage == pagePalettes && page != pagePalettes) {
        SetPalette(gMainWindow, gOriginalPalette, true);
        ActivatePalette(gMainWindow);
    }
    gPage = page;
    if (page == pagePalettes) {
        SetPalette(gMainWindow, gShowcasePalette, true);
        ActivatePalette(gMainWindow);
    }
    ShowAllControls(page);
    SyncMenuState();

    if (page == pageWindows && gAuxWindow == nil) {
        SetRect(&bounds, 180, 155, 570, 300);
        gAuxWindow = NewCWindow(nil, &bounds, "\pAuxiliary Window", true,
                                documentProc, (WindowPtr)-1, true, 0);
        CheckItem(StateMenu(), iWindowState, gAuxWindow != nil);
        DrawAuxWindow();
    } else if (page != pageWindows && gAuxWindow != nil) {
        DisposeWindow(gAuxWindow);
        gAuxWindow = nil;
        CheckItem(StateMenu(), iWindowState, false);
    }

    DrawMainWindow();
}

static void Initialize(void)
{
    Handle menuBar;
    MenuHandle hMenu;
    Rect r;

    InitGraf(&qd.thePort);
    InitFonts();
    InitWindows();
    InitMenus();
    TEInit();
    InitDialogs(nil);
    InitCursor();

    menuBar = GetNewMBar(rMenuBar);
    if (menuBar == nil) {
        ExitToShell();
    }
    SetMenuBar(menuBar);
    DisposeHandle(menuBar);

    /* Insert hierarchical submenus into the hierarchical partition (-1) */
    hMenu = GetMenu(mOptions);
    if (hMenu != nil) InsertMenu(hMenu, -1);
    hMenu = GetMenu(mDifficulty);
    if (hMenu != nil) InsertMenu(hMenu, -1);
    hMenu = GetMenu(mSoundMenu);
    if (hMenu != nil) InsertMenu(hMenu, -1);
    hMenu = GetMenu(mRendererMenu);
    if (hMenu != nil) InsertMenu(hMenu, -1);

    DrawMenuBar();

    gMainWindow = GetNewCWindow(rMainWindow, nil, (WindowPtr)-1);
    if (gMainWindow == nil) {
        ExitToShell();
    }
    SetPort(gMainWindow);
    ShowWindow(gMainWindow);

    gOriginalPalette = GetPalette(gMainWindow);
    if (gOriginalPalette == nil) {
        gOriginalPalette = GetNewPalette(0);
    }
    gShowcasePalette = GetNewPalette(rShowcasePalette);
    SetPalette(gMainWindow, gOriginalPalette, true);
    ActivatePalette(gMainWindow);

    /* Page 2: Controls */
    SetRect(&r, 40, 255, 150, 279);
    gButton = NewControl(gMainWindow, &r, "\pActivate", false, 0, 0, 1,
                         pushButProc, 0);
    SetRect(&r, 185, 255, 315, 279);
    gCheckbox = NewControl(gMainWindow, &r, "\pCheckbox", false, 0, 0, 1,
                           checkBoxProc, 0);
    SetRect(&r, 40, 310, 500, 326);
    gScrollbar = NewControl(gMainWindow, &r, "\p", false, 0, 0, 10,
                            scrollBarProc, 0);

    /* Page 5: Game Preferences Controls */
    SetRect(&r, 35, 70, 210, 90);
    gPrefSndFX = NewControl(gMainWindow, &r, "\pSound Effects (SFX)", false, 1, 0, 1,
                            checkBoxProc, 0);
    SetRect(&r, 35, 95, 210, 115);
    gPrefMusic = NewControl(gMainWindow, &r, "\pBackground Music", false, 1, 0, 1,
                            checkBoxProc, 0);
    SetRect(&r, 35, 195, 215, 211);
    gPrefVolume = NewControl(gMainWindow, &r, "\p", false, 75, 0, 100,
                             scrollBarProc, 0);

    SetRect(&r, 250, 70, 390, 90);
    gPrefDiffEasy = NewControl(gMainWindow, &r, "\pRecruit (Easy)", false, 0, 0, 1,
                               radioButProc, 0);
    SetRect(&r, 250, 95, 390, 115);
    gPrefDiffNormal = NewControl(gMainWindow, &r, "\pVeteran (Normal)", false, 1, 0, 1,
                                 radioButProc, 0);
    SetRect(&r, 250, 120, 390, 140);
    gPrefDiffHard = NewControl(gMainWindow, &r, "\pNightmare (Hard)", false, 0, 0, 1,
                               radioButProc, 0);

    SetRect(&r, 250, 195, 420, 215);
    gPrefRendFlat = NewControl(gMainWindow, &r, "\pClassic 2D Flat", false, 0, 0, 1,
                               radioButProc, 0);
    SetRect(&r, 250, 215, 420, 235);
    gPrefRendBevel = NewControl(gMainWindow, &r, "\pQD3D Bevels (Emul.)", false, 1, 0, 1,
                                radioButProc, 0);
    SetRect(&r, 250, 235, 420, 255);
    gPrefRendContrast = NewControl(gMainWindow, &r, "\pHigh Contrast", false, 0, 0, 1,
                                   radioButProc, 0);

    SetRect(&r, 35, 315, 145, 339);
    gPrefBtnApply = NewControl(gMainWindow, &r, "\pSave & Apply", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 160, 315, 270, 339);
    gPrefBtnReset = NewControl(gMainWindow, &r, "\pReset Defaults", false, 0, 0, 1,
                               pushButProc, 0);
    SetRect(&r, 285, 315, 425, 339);
    gPrefBtnModal = NewControl(gMainWindow, &r, "\pModal Dialog…", false, 0, 0, 1,
                               pushButProc, 0);

    /* Page 6: Dialogs Controls */
    SetRect(&r, 40, 305, 220, 329);
    gDlgBtnOpenPrefs = NewControl(gMainWindow, &r, "\pOpen Modal Dialog…", false, 0, 0, 1,
                                  pushButProc, 0);
    SetRect(&r, 240, 305, 410, 329);
    gDlgBtnOpenAlert = NewControl(gMainWindow, &r, "\pDisplay About Alert…", false, 0, 0, 1,
                                  pushButProc, 0);

    /* Page 7: Palette Manager Control */
    SetRect(&r, 40, 335, 190, 361);
    gPaletteAnimate = NewControl(gMainWindow, &r, "\pAnimate Palette", false, 0, 0, 1,
                                 pushButProc, 0);

    SetPage(pageGraphics);
}

static void DoMenuChoice(long choice)
{
    short menuID;
    short item;

    menuID = HiWord(choice);
    item = LoWord(choice);

    if (menuID == mPages && item >= iGraphics && item <= iPalettes) {
        SetPage(item);
    } else if (menuID == mDifficulty) {
        gDifficulty = item;
        SyncMenuState();
        DrawMainWindow();
    } else if (menuID == mSoundMenu) {
        if (item == iSndMute) {
            gSoundFX = false;
            gMusic = false;
        } else if (item == iSndFXOnly) {
            gSoundFX = true;
            gMusic = false;
        } else if (item == iSndMusicOnly) {
            gSoundFX = false;
            gMusic = true;
        } else if (item == iSndFull) {
            gSoundFX = true;
            gMusic = true;
        }
        SyncMenuState();
        DrawMainWindow();
    } else if (menuID == mRendererMenu) {
        gRenderer = item;
        SyncMenuState();
        DrawMainWindow();
    } else if (menuID == mOptions) {
        if (item == iOptResetPrefs) {
            gDifficulty = iDiffNormal;
            gSoundFX = true;
            gMusic = true;
            gVolume = 75;
            gRenderer = iRendBevel;
            SyncMenuState();
            DrawMainWindow();
        } else if (item == iOptLaunchDialog) {
            DoModalPrefsDialog();
        }
    } else if (menuID == mFile) {
        if (item == iFilePrefs) {
            SetPage(pagePreferences);
        } else if (item == iQuit) {
            gQuit = true;
        }
    } else if (menuID == mApple) {
        if (item == iAbout) {
            DoAboutAlert();
        }
    }
    HiliteMenu(0);
}

static void DoContentClick(WindowPtr window, Point where)
{
    ControlHandle control;
    short part;
    short trackedPart;
    short value;

    if (window != gMainWindow) {
        return;
    }
    SetPort(window);
    GlobalToLocal(&where);
    part = FindControl(where, window, &control);
    if (part == 0) {
        return;
    }
    trackedPart = TrackControl(control, where, nil);
    if (trackedPart == 0) {
        return;
    }
    part = trackedPart;

    if (control == gButton) {
        gButtonActivated = !gButtonActivated;
        CheckItem(StateMenu(), iButtonState, gButtonActivated);
    } else if (control == gCheckbox) {
        value = GetControlValue(gCheckbox) == 0 ? 1 : 0;
        SetControlValue(gCheckbox, value);
        CheckItem(StateMenu(), iCheckboxState, value != 0);
    } else if (control == gScrollbar) {
        value = GetControlValue(gScrollbar);
        if (part == kControlUpButtonPart || part == kControlPageUpPart) {
            value -= part == kControlPageUpPart ? 3 : 1;
        } else if (part == kControlDownButtonPart || part == kControlPageDownPart) {
            value += part == kControlPageDownPart ? 3 : 1;
        }
        if (value < 0) value = 0;
        if (value > 10) value = 10;
        SetControlValue(gScrollbar, value);
        CheckItem(StateMenu(), iScrollbarState,
                  GetControlValue(gScrollbar) != 0);
    } else if (control == gPrefSndFX) {
        gSoundFX = !gSoundFX;
        SyncMenuState();
    } else if (control == gPrefMusic) {
        gMusic = !gMusic;
        SyncMenuState();
    } else if (control == gPrefVolume) {
        value = GetControlValue(gPrefVolume);
        if (part == kControlUpButtonPart || part == kControlPageUpPart) {
            value -= part == kControlPageUpPart ? 15 : 5;
        } else if (part == kControlDownButtonPart || part == kControlPageDownPart) {
            value += part == kControlPageDownPart ? 15 : 5;
        }
        if (value < 0) value = 0;
        if (value > 100) value = 100;
        gVolume = value;
        SetControlValue(gPrefVolume, gVolume);
    } else if (control == gPrefDiffEasy) {
        gDifficulty = iDiffEasy;
        SyncMenuState();
    } else if (control == gPrefDiffNormal) {
        gDifficulty = iDiffNormal;
        SyncMenuState();
    } else if (control == gPrefDiffHard) {
        gDifficulty = iDiffHard;
        SyncMenuState();
    } else if (control == gPrefRendFlat) {
        gRenderer = iRendFlat;
        SyncMenuState();
    } else if (control == gPrefRendBevel) {
        gRenderer = iRendBevel;
        SyncMenuState();
    } else if (control == gPrefRendContrast) {
        gRenderer = iRendContrast;
        SyncMenuState();
    } else if (control == gPrefBtnApply) {
        /* Save / Apply feedback */
        SyncMenuState();
    } else if (control == gPrefBtnReset) {
        gDifficulty = iDiffNormal;
        gSoundFX = true;
        gMusic = true;
        gVolume = 75;
        gRenderer = iRendBevel;
        SyncMenuState();
    } else if (control == gPrefBtnModal || control == gDlgBtnOpenPrefs) {
        DoModalPrefsDialog();
        return;
    } else if (control == gDlgBtnOpenAlert) {
        DoAboutAlert();
        return;
    } else if (control == gPaletteAnimate) {
        AnimateShowcasePalette();
        return;
    }
    DrawMainWindow();
}

static void DoEvent(EventRecord *event)
{
    WindowPtr window;
    short part;
    char key;

    switch (event->what) {
        case mouseDown:
            part = FindWindow(event->where, &window);
            if (part == inMenuBar) {
                DoMenuChoice(MenuSelect(event->where));
            } else if (part == inContent) {
                if (window != FrontWindow()) {
                    SelectWindow(window);
                } else {
                    DoContentClick(window, event->where);
                }
            } else if (part == inDrag) {
                DragWindow(window, event->where, &qd.screenBits.bounds);
            } else if (part == inGoAway && TrackGoAway(window, event->where)) {
                if (window == gAuxWindow) {
                    DisposeWindow(gAuxWindow);
                    gAuxWindow = nil;
                    CheckItem(StateMenu(), iWindowState, false);
                } else {
                    gQuit = true;
                }
            }
            break;

        case keyDown:
        case autoKey:
            key = (char)(event->message & charCodeMask);
            if ((event->modifiers & cmdKey) != 0) {
                DoMenuChoice(MenuKey(key));
            }
            break;

        case updateEvt:
            window = (WindowPtr)event->message;
            BeginUpdate(window);
            if (window == gMainWindow) {
                DrawMainWindow();
            } else if (window == gAuxWindow) {
                DrawAuxWindow();
            }
            EndUpdate(window);
            break;
    }
}

void main(void)
{
    EventRecord event;

    Initialize();
    while (!gQuit) {
        if (WaitNextEvent(everyEvent, &event, 1, nil)) {
            DoEvent(&event);
        }
    }
    SetPalette(gMainWindow, nil, true);
    if (gShowcasePalette != nil) DisposePalette(gShowcasePalette);
    if (gOriginalPalette != nil) DisposePalette(gOriginalPalette);
    ExitToShell();
}
