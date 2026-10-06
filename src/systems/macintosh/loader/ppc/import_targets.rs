//! CFM import dispatch targets and symbol resolution tables.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PpcImportDispatcherTarget {
    Collection(PpcCollectionOperation),
    InstallExceptionHandler,
    NewPtr { clear: bool },
    DisposePtr,
    GetPtrSize,
    SetPtrSize,
    RecoverHandle,
    BlockMove,
    BlockZero,
    PtrToHand,
    PtrToXHand,
    HandToHand,
    HandAndHand,
    NewHandle { clear: bool },
    TempNewHandle,
    TempDisposeHandle,
    HoldMemory,
    UnholdMemory,
    DisposeHandle,
    EmptyHandle,
    GetHandleSize,
    SetHandleSize,
    HLock,
    HLockHi,
    HGetState,
    HSetState,
    HUnlock,
    MoveHHi,
    HNoPurge,
    HPurge,
    TickCount,
    GetZone,
    SetZone,
    InitZone,
    SystemZone,
    ApplicationZone,
    MaxApplZone,
    MoreMasters,
    FlushCodeCache,
    GetApplLimit,
    SetApplLimit,
    HeapFreeBytes,
    MaxMem,
    PurgeMem,
    PurgeMemSys,
    MemError,
    CurResFile,
    UseResFile,
    CloseResFile,
    OpenResFile,
    HOpenResFile,
    ResError,
    SetResLoad,
    LMGetResLoad,
    LoadResource,
    GetIndString,
    GetString,
    GetResource,
    Get1Resource,
    GetNamedResource,
    Get1NamedResource,
    GetIndResource,
    Get1IndResource,
    GetResAttrs,
    SetResAttrs,
    GetResInfo,
    GetResourceSizeOnDisk,
    SetResInfo,
    HomeResFile,
    CountResources,
    Count1Resources,
    CountTypes,
    Count1Types,
    GetIndType,
    Get1IndType,
    UniqueID,
    Unique1ID,
    UpdateResFile,
    AddResource,
    ChangedResource,
    WriteResource,
    RemoveResource,
    ReleaseResource,
    DetachResource,
    ReadPartialResource,
    GetPicture,
    GetIcon,
    GetIconSuite,
    GetPattern,
    GetIndPattern,
    GetPixPat,
    GetPictInfo,
    DrawPicture,
    KillPicture,
    Gestalt,
    NewGestaltValue,
    RegisterAppearanceClient,
    ActivateControl,
    DeactivateControl,
    IsControlActive,
    CollapseWindow,
    IsWindowCollapsed,
    UnregisterAppearanceClient,
    SetControlFontStyle,
    GetSharedLibrary,
    FindSymbol,
    CountSymbols,
    GetIndSymbol,
    CloseConnection,
    GetMemFragment,
    GetDiskFragment,
    InitCursor,
    GetQDGlobalsArrow,
    HideCursor,
    ShowCursor,
    ShieldCursor,
    CrsrDevNextDevice,
    CrsrDevMoveTo,
    GetCursor,
    SetCursor,
    GetCCursor,
    GetCIcon,
    PlotCIcon,
    DisposeCIcon,
    SetCCursor,
    DisposeCCursor,
    SysBeep,
    GetForeColor,
    GetBackColor,
    ForeColor,
    BackColor,
    RGBForeColor,
    RGBBackColor,
    OpColor,
    HiliteColor,
    PmForeColor,
    PmBackColor,
    Color2Index,
    Index2Color,
    RGB2HSL,
    RGB2HSV,
    HSV2RGB,
    FixRatio,
    FixMul,
    FixDiv,
    Long2Fix,
    Fix2Long,
    FixRound,
    Fix2Frac,
    Frac2Fix,
    Frac2X,
    X2Frac,
    FracSin,
    FracCos,
    FracSqrt,
    FracMul,
    FracDiv,
    FixATan2,
    WideAdd,
    WideSubtract,
    WideNegate,
    WideShift,
    WideBitShift,
    WideMultiply,
    WideDivide,
    WideWideDivide,
    WideSquareRoot,
    WideCompare,
    MoveTo,
    MovePortTo,
    PortSize,
    Move,
    LineTo,
    Line,
    DrawChar,
    DrawText,
    DrawString,
    TextFont,
    TextFace,
    TextMode,
    TextSize,
    CharExtra,
    PaintRect,
    EraseRect,
    InvertRect,
    FrameRect,
    FillRect,
    FrameOval,
    PaintOval,
    EraseOval,
    PaintArc,
    FrameRgn,
    PaintRgn,
    FillRgn,
    InvertRgn,
    FillCRect,
    FrameRoundRect,
    PaintRoundRect,
    InvalRect,
    InvalRgn,
    ValidRect,
    ValidRgn,
    BeginUpdate,
    EndUpdate,
    ClipRect,
    GetClip,
    SetClip,
    GetPen,
    HidePen,
    ShowPen,
    PenSize,
    PenMode,
    PenNormal,
    PenPixPat,
    GetPenState,
    SetPenState,
    CopyBits,
    UnpackBits,
    BitMapToRegion,
    NewRgn,
    DisposeRgn,
    CopyRgn,
    OpenRgn,
    CloseRgn,
    SectRgn,
    UnionRgn,
    DiffRgn,
    XorRgn,
    SetEmptyRgn,
    SetRectRgn,
    RectRgn,
    OffsetRgn,
    EmptyRgn,
    PtInRgn,
    RectInRgn,
    OpenPoly,
    ClosePoly,
    KillPoly,
    PaintPoly,
    FramePoly,
    FillPoly,
    NewCWindow,
    GetNewCWindow,
    GetWRefCon,
    SetWRefCon,
    GetWindowPic,
    SetWindowPic,
    GetAuxWin,
    LMGetWindowList,
    LMSetWindowList,
    LMSetCurActivate,
    LMGetAuxWinHead,
    LMSetAuxWinHead,
    SizeWindow,
    MoveWindow,
    ShowWindow,
    HideWindow,
    ShowHide,
    CloseWindow,
    SelectWindow,
    FrontWindow,
    SetWinColor,
    PaintOne,
    PaintBehind,
    CalcVisBehind,
    GetMouse,
    ActivatePalette,
    NSetPalette,
    GetPalette,
    GetPort,
    GetPortBounds,
    GetWMgrPort,
    SetPort,
    GetGDevice,
    SetGDevice,
    GetDeviceList,
    GetNextDevice,
    GetMainDevice,
    GetMaxDevice,
    DeviceLoop,
    GetSysFont,
    GetAppFont,
    GetDefFontSize,
    GetFontName,
    GetMBarHeight,
    SetMBarHeight,
    NewMenu,
    DisposeMenu,
    GetMenu,
    GetItemCmd,
    SetItemCmd,
    GetItemMark,
    CountMItems,
    GetMenuItemText,
    SetMenuItemText,
    DeleteMenuItem,
    CalcMenuSize,
    PopUpMenuSelect,
    InsertMenu,
    DeleteMenu,
    AppendMenu,
    InsertMenuItem,
    AppendResMenu,
    InsertResMenu,
    EnableMenuItem,
    DisableMenuItem,
    SetItemMark,
    CheckItem,
    GetMenuBar,
    GetNewMBar,
    LMGetMenuList,
    LMSetMenuHook,
    LMGetMenuFlash,
    LMGetPaintWhite,
    LMGetSysMap,
    LMGetCurApRefNum,
    GetVCBQHdr,
    GetDrvQHdr,
    LMGetSysEvtMask,
    LMSetSysEvtMask,
    LMGetDefltStack,
    LMGetCurStackBase,
    LMSetPaintWhite,
    LMSetResumeProc,
    LMGetResumeProc,
    LMSetACount,
    LMGetACount,
    LMSetANumber,
    LMGetANumber,
    LMSetDABeeper,
    LMGetDABeeper,
    LMGetDAStrings,
    LMSetDlgFont,
    LMGetDlgFont,
    SetMenuFlash,
    ClearMenuBar,
    InvalMenuBar,
    SetMenuBar,
    GetMenuHandle,
    DrawMenuBar,
    FlashMenuBar,
    HMGetHelpMenuHandle,
    HMGetBalloons,
    HMIsBalloon,
    HMSetBalloons,
    HiliteMenu,
    DrawGrowIcon,
    MenuNoop,
    MenuKey,
    MenuEvent,
    MenuChoice,
    MenuSelect,
    TestDeviceAttribute,
    SetDeviceAttribute,
    HasDepth,
    SetDepth,
    NewGWorld,
    UpdateGWorld,
    DisposeGWorld,
    GetCTable,
    GetCTSeed,
    MakeITable,
    QDError,
    QDDone,
    CTabChanged,
    ProtectEntry,
    ReserveEntry,
    RestoreEntries,
    SetEntries,
    RestoreDeviceClut,
    DisposeCTable,
    NewPixMap,
    DisposePixMap,
    GetGWorld,
    SetGWorld,
    GetWindowPort,
    SetPortWindowPort,
    GetGWorldDevice,
    GetGWorldPixMap,
    OpenPort,
    OpenCPort,
    InitCPort,
    CloseCPort,
    SetPortBits { color: bool },
    GetPixBaseAddr,
    GetPixRowBytes,
    LockPixels,
    UnlockPixels,
    GetPixelsState,
    SetPixelsState,
    AllowPurgePixels,
    NoPurgePixels,
    SetRect,
    SectRect,
    UnionRect,
    EqualRect,
    EmptyRect,
    SetPt,
    EqualPt,
    AddPt,
    SubPt,
    LocalToGlobal,
    GlobalToLocal,
    PtInRect,
    SetOrigin,
    OffsetRect,
    MapRect,
    InsetRect,
    FindWindow,
    PinRect,
    GetWVariant,
    ClipAbove,
    SaveOld,
    DrawNew,
    DragGrayRgn,
    GetGrayRgn,
    LMSetGrayRgn,
    GetDCtlEntry,
    GetADBInfo,
    AutoSleepControl,
    IsAutoSlpControlDisabled,
    OpenDriver,
    Control,
    PBControl,
    PBStatus,
    FindFolder,
    NewAlias,
    NewAliasMinimalFromFullPath,
    UpdateAlias,
    ResolveAlias,
    ResolveAliasFile,
    ResolveAliasFileWithMountFlags,
    GetIconRefFromFile,
    GetIconRef,
    PlotIconRef,
    ReleaseIconRef,
    DirCreate,
    FSpDirCreate,
    FSMakeFSSpec,
    PBGetFInfo,
    PBHGetFInfo,
    PBSetFInfo,
    PBHSetFInfo,
    PBGetCatInfo,
    PBSetCatInfo,
    PBHGetVInfo,
    GetVInfo,
    GetVRefNum,
    PBDTGetPath,
    PBDTGetCommentSync,
    PBGetFCBInfo,
    FSpGetFInfo,
    GetFInfo,
    HGetFInfo,
    FSpSetFInfo,
    HSetFInfo,
    StandardGetFile,
    GetScrap,
    PutScrap,
    ZeroScrap,
    LoadScrap,
    UnloadScrap,
    SndSoundManagerVersion,
    UnsignedFixedMulDiv,
    GetSoundOutputInfo,
    GetCompressionInfo,
    GetSoundVol,
    SetSoundVol,
    GetDefaultOutputVolume,
    SetDefaultOutputVolume,
    GetVol,
    GetWDInfo,
    OpenWD,
    SetVol,
    HGetVol,
    HSetVol,
    FlushVol,
    PBFlushVol,
    SndNewChannel,
    SetupSndHeader,
    SndDisposeChannel,
    SndPlay,
    SndChannelStatus,
    SndGetInfo,
    SndSetInfo,
    ParseSndHeader,
    SndDoCommand,
    SndDoImmediate,
    SndPlayDoubleBuffer,
    SndStartFilePlay,
    SndPauseFilePlay,
    SndStopFilePlay,
    GetSoundHeaderOffset,
    FSpCreateResFile,
    HCreateResFile,
    FSpOpenDF,
    FSpOpenRF,
    PBOpen,
    PBHOpenDF,
    HOpen,
    FSOpen,
    FSpOpenResFile,
    FSClose,
    PBClose,
    PBFlushFile,
    FSRead,
    PBRead,
    FSWrite,
    PBWrite,
    GetEOF,
    PBGetEOF,
    SetEOF,
    AllocContig,
    PBSetEOF,
    GetFPos,
    SetFPos,
    PBSetFPos,
    FSpCreate,
    PBCreate(PpcParameterBlockCreateOperation),
    FSpDelete,
    DeleteByName(PpcDeleteByNameOperation),
    HCreate,
    HRename,
    Create,
    DSpGetFirstContext,
    DSpGetNextContext,
    DSpFindBestContext,
    DSpFindBestContextOnDisplayID,
    DSpUserSelectContext,
    DSpStartup,
    DSpGetVersion,
    DSpShutdown,
    DSpProcessEvent,
    DSpBlitFastest,
    DSpCanUserSelectContext,
    DSpGetMouse,
    DSpFindContextFromPoint,
    DSpContextGlobalToLocal,
    DSpContextLocalToGlobal,
    DSpSetBlankingColor,
    DSpAltBufferNew,
    DSpAltBufferGetCGrafPtr,
    DSpContextReserve,
    DSpContextRelease,
    DSpContextSetState,
    DSpContextGetState,
    DSpContextFadeGamma,
    DSpContextFadeGammaIn,
    DSpContextFadeGammaOut,
    DSpContextGetFrontBuffer,
    DSpContextGetBackBuffer,
    DSpContextSwapBuffers,
    DSpContextSetClutEntries,
    DSpContextGetClutEntries,
    DSpContextGetDisplayID,
    DSpContextGetAttributes,
    DSpContextGetFlattenedSize,
    DSpContextFlatten,
    DSpContextRestore,
    DSpContextSetVblProc,
    DSpContextIsBusy,
    DSpAltBufferDispose,
    DSpContextInvalBackBufferRect,
    DSpContextSetUnderlayAltBuffer,
    DMGetDisplayIDByGDevice,
    DMGetNameByAVID,
    DMGetGDeviceByDisplayID,
    GetNewDialog,
    NewDialog,
    NewFeaturesDialog,
    GetDialogItem,
    GetDialogItemAsControl,
    SetDialogItem,
    GetDialogItemText,
    SetDialogItemText,
    SetDialogDefaultItem,
    GetDialogDefaultItem,
    SetDialogCancelItem,
    GetDialogCancelItem,
    SetDialogTracksCursor,
    MoveDialogItem,
    SizeDialogItem,
    AppendDialogItemList,
    AutoSizeDialog,
    CouldDialog,
    FreeDialog,
    CouldAlert,
    FreeAlert,
    StdFilterProc,
    GetStdFilterProc,
    GetAlertStage,
    SetDialogFont,
    GetDialogPort,
    GetDialogWindow,
    GetDialogFromWindow,
    SetPortDialogPort,
    DrawDialog,
    DrawControls,
    UpdateControls,
    ModalDialog,
    SetControlTitle,
    SetControlValue,
    HiliteControl,
    InitGraf,
    InitFonts,
    InitWindows,
    InitMenus,
    TEInit,
    TENew,
    TEStyleNew,
    TESetStyle,
    TEUseStyleScrap,
    TEContinuousStyle,
    MeasureText,
    TEGetText,
    TEDispose,
    TEActivate { active: bool },
    TESetSelect,
    TESetText,
    TECalText,
    TEInsert { styled: bool },
    TEDelete { dialog: bool },
    TEKey,
    TEClick,
    TEIdle,
    TEUpdate,
    TETextBox,
    TESetAlignment,
    TEGetHeight,
    TEGetPoint,
    TEScroll { pinned: bool },
    TEAutoView,
    TECopy { cut: bool, dialog: bool },
    TEPaste { dialog: bool },
    TETransferScrap { from_desktop: bool },
    TEScrapHandle,
    TEScrapLength { set: bool },
    SelectDialogItemText,
    InitDialogs,
    ErrorSound,
    SystemTask,
    SystemClick,
    OpenDeskAcc,
    AEInstallEventHandler,
    AEProcessAppleEvent,
    LNew,
    LDispose,
    LAddRow,
    LDelRow,
    LGetSelect,
    LSetSelect,
    LSetCell,
    LGetCell,
    LClick,
    LActivate,
    LSetDrawingMode,
    LScroll,
    LSize,
    LUpdate,
    LAutoScroll,
    LSearch,
    FlushEvents,
    GetMainEventQueue,
    GetMainEventLoop,
    InstallEventLoopTimer,
    RemoveEventLoopTimer,
    GetApplicationEventTarget,
    GetEventDispatcherTarget,
    InstallEventHandler,
    RemoveEventHandler,
    CreateEvent,
    ReleaseEvent,
    RetainEvent,
    GetEventClass,
    GetEventKind,
    GetEventTime,
    SetEventParameter,
    GetEventParameter,
    PostEventToQueue,
    ReceiveNextEvent,
    SendEventToEventTarget,
    CallNextEventHandler,
    RunApplicationEventLoop,
    QuitApplicationEventLoop,
    InstallStandardEventHandler,
    GetCurrentEventTime,
    FlushEventQueue,
    SetEventMask,
    CloseDialog,
    DisposeDialog,
    GetNextEvent(PpcEventPollOperation),
    GetOSEvent,
    EventAvail,
    OSEventAvail,
    PostEvent,
    Button,
    StillDown,
    WaitMouseUp,
    GetKeys,
    GetDateTime,
    ReadDateTime,
    ReadLocation,
    GetTime,
    Delay,
    GetDblTime,
    LMGetTime,
    LMGetUTableBase,
    SecondsToDate,
    Microseconds,
    AbsoluteToNanoseconds,
    SysEnvirons,
    TextWidth,
    StringWidth,
    TruncString,
    CharWidth,
    RealFont,
    GetFontInfo,
    FontMetrics,
    GetFNum,
    GetIntlResource,
    AESetInteractionAllowed,
    AEGetInteractionAllowed,
    AEManagerInfo,
    LMGetCurDirStore,
    LMSetCurDirStore,
    LMGetSFSaveDisk,
    LMSetSFSaveDisk,
    LMGetRndSeed,
    LMSetRndSeed,
    SetCurrentA5,
    SetA5,
    SVersion,
    DMGetFirstScreenDevice,
    DMGetNextScreenDevice,
    DMGetDisplayMode,
    DMCheckDisplayMode,
    DMSetDisplayMode,
    DMNewDisplayModeList,
    DMGetIndexedDisplayModeFromList,
    DMDisposeList,
    DMBeginConfigureDisplays,
    DMEndConfigureDisplays,
    SetToolTrapAddress,
    SetOSTrapAddress,
    NSetTrapAddress,
    EqualString,
    IUEqualPString,
    GetIntlResourceTable,
    NumToString,
    StringToNum,
    Random,
    BitAnd,
    BitOr,
    BitTst,
    StdMemset,
    StdMemcmp,
    StdMemcpy,
    StdMemmove,
    StdMalloc,
    StdFree,
    StdCalloc,
    StdRealloc,
    StdStrcpy,
    StdStrcat,
    StdStrncpy,
    StdStrncat,
    StdStrcmp,
    StdStrncmp,
    StdStrlen,
    StdMemchr,
    StdStrchr,
    StdStrrchr,
    StdStrspn,
    StdStrcspn,
    StdStrpbrk,
    StdStrstr,
    StdAtoi,
    StdGetenv,
    StdSprintf,
    StdIoCompatibility(PpcStdIoOperation),
    StdAbs,
    StdToupper,
    StdTolower,
    StdIsalnum,
    StdIsalpha,
    StdIsascii,
    StdIscntrl,
    StdIsdigit,
    StdIsgraph,
    StdIslower,
    StdIsprint,
    StdIspunct,
    StdIsspace,
    StdIsupper,
    StdIsxdigit,
    StdToascii,
    StdSrand,
    StdRand,
    StdTime,
    P2CStr,
    C2PStr,
    CopyCStringToPascal,
    CopyPascalStringToC,
    CfStringMakeConstantString,
    CfStringCreateWithCString,
    CfStringCreateWithPascalString,
    CfStringCreateWithBytes,
    CfStringGetCString,
    CfStringGetBytes,
    CfStringGetLength,
    CfStringGetSystemEncoding,
    CfRetain,
    CfRelease,
    CfGetRetainCount,
    CfBundleGetBundleWithIdentifier,
    CfBundleGetMainBundle,
    CfBundleCopyPrivateFrameworksUrl,
    CfUrlCreateCopyAppendingPathComponent,
    CfBundleCreate,
    CfBundleLoadExecutable,
    UpperText,
    GetCurrentThread,
    MpCreateSemaphore,
    MpDeleteSemaphore,
    MpSignalSemaphore,
    MpWaitOnSemaphore,
    NewThreadEntryUPP,
    DisposeThreadEntryUPP,
    NewThreadTerminationUPP,
    DisposeThreadTerminationUPP,
    NewThreadSwitchUPP,
    DisposeThreadSwitchUPP,
    SetThreadTerminator,
    SetThreadSwitcher,
    GetThreadState,
    GetThreadCurrentTaskRef,
    GetThreadStateGivenTaskRef,
    SetThreadReadyGivenTaskRef,
    SetThreadState,
    SetThreadStateEndCritical,
    ThreadBeginCritical,
    NewThread,
    CreateThreadPool,
    GetFreeThreadCount,
    GetSpecificFreeThreadCount,
    GetDefaultThreadStackSize,
    ThreadCurrentStackSpace,
    YieldToThread,
    YieldToAnyThread,
    DisposeThread,
    ThreadEndCritical,
    GetCurrentProcess,
    WakeUpProcess,
    SameProcess,
    GetProcessInformation,
    ParamText,
    AlertReturnDefault(crate::dialog_manager::AlertKind),
    StandardAlert,
    ExitToShell,
    MathCeil,
    MathSqrt,
    MathExp,
    MathSin,
    MathCos,
    MathRound,
    MathRint,
    MathAsin,
    MathTan,
    MathAtan,
    MathAtan2,
    MathPow,
    MathFmod,
    MathLog,
    MathLog10,
    MathDtox80,
    Math64(PpcMath64Operation),
    X2Fix,
    Q3MemoryStorageNew,
    Q3MemoryStorageNewBuffer,
    Q3FSSpecStorageNew,
    Q3MemoryStorageSet,
    Q3MemoryStorageGetBuffer,
    Q3MemoryStorageSetBuffer,
    Q3MemoryStorageGetType,
    Q3NewObject,
    Q3FileNew,
    Q3ViewNew,
    Q3Initialize,
    Q3Exit,
    Q3GetVersion,
    Q3DisplayGroupNew,
    Q3OrderedDisplayGroupNew,
    Q3ErrorGet,
    Q3ObjectDispose,
    Q3ObjectDuplicate,
    Q3SharedGetReference,
    Q3SharedIsReferenced,
    Q3SharedGetType,
    Q3ShapeGetType,
    Q3ShapeGetLeafType,
    Q3ObjectIsDrawable,
    Q3ObjectIsType,
    Q3ObjectGetType,
    Q3ObjectGetLeafType,
    Q3GeometryGetType,
    Q3ShaderGetType,
    Q3GroupGetType,
    Q3RendererNewFromType,
    Q3RendererGetType,
    Q3RendererSync,
    Q3RendererFlush,
    Q3InteractiveRendererSetDoubleBufferBypass,
    Q3InteractiveRendererSetPreferences,
    Q3InteractiveRendererSetRaveContextHints,
    Q3InteractiveRendererGetRaveContextHints,
    Q3InteractiveRendererGetRaveDrawContexts,
    Q3InteractiveRendererSetRaveTextureFilter,
    Q3TextureShaderNew,
    Q3TextureShaderGetTexture,
    Q3LambertIlluminationNew,
    Q3NullIlluminationNew,
    Q3PhongIlluminationNew,
    Q3MipmapTextureNew,
    Q3MipmapTextureGetMipmap,
    Q3ViewAngleAspectCameraNew,
    Q3OrthographicCameraNew,
    Q3ViewPlaneCameraNew,
    Q3CameraGetPlacement,
    Q3CameraSetPlacement,
    Q3CameraGetRange,
    Q3CameraSetRange,
    Q3CameraGetViewPort,
    Q3CameraSetViewPort,
    Q3CameraGetWorldToView,
    Q3CameraGetViewToFrustum,
    Q3LightGetType,
    Q3LightGetState,
    Q3LightSetState,
    Q3LightGetBrightness,
    Q3LightSetBrightness,
    Q3LightGetColor,
    Q3LightSetColor,
    Q3LightGetData,
    Q3LightSetData,
    Q3AmbientLightNew,
    Q3AmbientLightGetData,
    Q3AmbientLightSetData,
    Q3DirectionalLightNew,
    Q3DirectionalLightGetCastShadowsState,
    Q3DirectionalLightSetCastShadowsState,
    Q3DirectionalLightGetDirection,
    Q3DirectionalLightSetDirection,
    Q3DirectionalLightGetData,
    Q3DirectionalLightSetData,
    Q3PointLightNew,
    Q3PointLightGetCastShadowsState,
    Q3PointLightSetCastShadowsState,
    Q3PointLightGetAttenuation,
    Q3PointLightSetAttenuation,
    Q3PointLightGetLocation,
    Q3PointLightSetLocation,
    Q3PointLightGetData,
    Q3PointLightSetData,
    Q3SpotLightNew,
    Q3SpotLightGetCastShadowsState,
    Q3SpotLightSetCastShadowsState,
    Q3SpotLightGetAttenuation,
    Q3SpotLightSetAttenuation,
    Q3SpotLightGetLocation,
    Q3SpotLightSetLocation,
    Q3SpotLightGetDirection,
    Q3SpotLightSetDirection,
    Q3SpotLightGetHotAngle,
    Q3SpotLightSetHotAngle,
    Q3SpotLightGetOuterAngle,
    Q3SpotLightSetOuterAngle,
    Q3SpotLightGetFallOff,
    Q3SpotLightSetFallOff,
    Q3SpotLightGetData,
    Q3SpotLightSetData,
    Q3LightGroupNew,
    Q3ShaderGetUVTransform,
    Q3ShaderSetUVTransform,
    Q3ShaderGetUBoundary,
    Q3ShaderSetUBoundary,
    Q3ShaderGetVBoundary,
    Q3ShaderSetVBoundary,
    Q3BackfacingStyleNew,
    Q3BackfacingStyleGet,
    Q3BackfacingStyleSet,
    Q3InterpolationStyleNew,
    Q3InterpolationStyleGet,
    Q3InterpolationStyleSet,
    Q3FillStyleNew,
    Q3FillStyleGet,
    Q3FillStyleSet,
    Q3OrientationStyleNew,
    Q3OrientationStyleGet,
    Q3OrientationStyleSet,
    Q3TriMeshNew,
    Q3TriMeshGetData,
    Q3TriMeshSetData,
    Q3TriMeshEmptyData,
    Q3AttributeSetNew,
    Q3AttributeSetAdd,
    Q3AttributeSetGet,
    Q3AttributeSetClear,
    Q3AttributeSetContains,
    Q3AttributeSetGetNextAttributeType,
    Q3StorageGetSize,
    Q3StorageGetData,
    Q3StorageSetData,
    Q3StorageGetType,
    Q3PixmapDrawContextNew,
    Q3MacDrawContextNew,
    Q3DrawContextGetPane,
    Q3Vector3DNormalize,
    Q3Vector3DLength,
    Q3Vector2DNormalize,
    Q3Vector3DCross,
    Q3Point2DDistance,
    Q3Point3DDistance,
    Q3Point3DCrossProductTri,
    Q3BoundingBoxSetFromPoints3D,
    Q3Matrix3x3SetTranslate,
    Q3Matrix4x4SetIdentity,
    Q3Matrix4x4SetTranslate,
    Q3Matrix4x4SetScale,
    Q3Matrix4x4SetRotateX,
    Q3Matrix4x4SetRotateY,
    Q3Matrix4x4SetRotateZ,
    Q3Matrix4x4SetRotateXyz,
    Q3Matrix4x4Multiply,
    Q3Matrix4x4Transpose,
    Q3Matrix4x4Invert,
    Q3Point3DTransform,
    Q3Point3DTo3DTransformArray,
    Q3Point3DTo4DTransformArray,
    Q3Vector3DTransform,
    Q3MatrixTransformNew,
    Q3MatrixTransformSet,
    Q3TransformGetMatrix,
    Q3FileSetStorage,
    Q3FileOpenRead,
    Q3FileReadObject,
    Q3FileIsEndOfFile,
    Q3FileClose,
    Q3GroupAddObject,
    Q3GroupAddObjectBefore,
    Q3GroupCountObjects,
    Q3GroupGetFirstPosition,
    Q3GroupGetNextPosition,
    Q3GroupGetFirstPositionOfType,
    Q3GroupGetPositionObject,
    Q3GroupRemovePosition,
    Q3ViewSetRenderer,
    Q3ViewGetRenderer,
    Q3ViewSetLightGroup,
    Q3ViewGetLightGroup,
    Q3ViewSetDrawContext,
    Q3ViewGetDrawContext,
    Q3ViewSetCamera,
    Q3ViewGetCamera,
    Q3ViewGetWorldToFrustumMatrixState,
    Q3ViewGetFrustumToWindowMatrixState,
    Q3ViewStartRendering,
    Q3ViewEndRendering,
    Q3ViewStartBoundingBox,
    Q3ViewEndBoundingBox,
    Q3ViewStartBoundingSphere,
    Q3ViewEndBoundingSphere,
    Q3ViewCancel,
    Q3ShaderSubmit,
    Q3StyleSubmit,
    Q3BackfacingStyleSubmit,
    Q3InterpolationStyleSubmit,
    Q3FillStyleSubmit,
    Q3OrientationStyleSubmit,
    Q3FogStyleSubmit,
    Q3TriMeshSubmit,
    Q3MatrixTransformSubmit,
    Q3ResetTransformSubmit,
    Q3PushSubmit,
    Q3PopSubmit,
    Q3ObjectSubmit,
    QADeviceGetFirstEngine,
    QADeviceGetNextEngine,
    QAEngineGestalt,
    ISpElementNewVirtualFromNeeds,
    ISpElementListNew,
    ISpElementListAddElements,
    ISpElementListGetNextEvent,
    ISpElementListFlush,
    ISpDevicesExtract,
    ISpDevicesExtractByClass,
    ISpDeviceGetDefinition,
    ISpDeviceGetElementList,
    ISpElementListExtract,
    ISpElementGetInfo,
    ISpElementGetConfigurationInfo,
    ISpElementGetSimpleState,
    ISpGetVersion,
    ISpStartup,
    ISpShutdown,
    ISpInit,
    ISpStop,
    ISpSuspend,
    ISpResume,
    ISpDevicesActivate,
    ISpDevicesDeactivate,
    ISpConfigure,
    GlideSstQueryBoards,
    QtEnterMovies,
    QtExitMovies,
    QtGetMoviesError,
    QtGetMoviesStickyError,
    QtClearMoviesStickyError,
    QtGetGraphicsImporterForFile,
    QtOpenADefaultComponent,
    QtGraphicsImportSetDataHandle,
    QtGraphicsImportGetImageDescription,
    QtGraphicsImportGetBoundsRect,
    QtGraphicsImportSetGWorld,
    QtGraphicsImportDraw,
    QtOpenMovieFile,
    QtNewMovieFromFile,
    QtGetMovieBox,
    QtSetMovieBox,
    QtSetMovieGWorld,
    QtStartMovie,
    QtStopMovie,
    QtMoviesTask,
    QtDisposeMovie,
    QtIsMovieDone,
    QtGoToBeginningOfMovie,
    QtGoToEndOfMovie,
    QtGetMovieDuration,
    QtLoadMovieIntoRam,
    QtCloseMovieFile,
    CloseComponent,
    NewOTNotifyUPP,
    NewRoutineDescriptor,
    NewIOCompletionUPP,
    DisposeIOCompletionUPP,
    NewControlUserPaneDrawUPP,
    DisposeControlUserPaneDrawUPP,
    NewAEEventHandlerUPP,
    DisposeAEEventHandlerUPP,
    NewEventHandlerUPP,
    DisposeEventHandlerUPP,
    NewEventLoopTimerUPP,
    DisposeEventLoopTimerUPP,
    NewControlActionUPP,
    DisposeControlActionUPP,
    NewControlKeyFilterUPP,
    DisposeControlKeyFilterUPP,
    NewControlEditTextValidationUPP,
    DisposeControlEditTextValidationUPP,
    NewFatRoutineDescriptor,
    DisposeRoutineDescriptor,
    CallUniversalProc,
    CallOSTrapUniversalProc,
    NGetTrapAddress,
    GetToolTrapAddress,
    GetOSTrapAddress,
    LMGetCurrentA5,
    InsTime,
    InsXTime,
    PrimeTime,
    RmvTime,
    VInstall,
    VRemove,
    SlotVInstall,
    SlotVRemove,
    LegacyMemoryUtility(PpcLegacyMemoryUtilityOperation),
    LegacyControl(PpcLegacyControlOperation),
    LegacyWindow(PpcLegacyWindowOperation),
    AppleEventCompatibility(PpcAppleEventCompatibilityOperation),
    DialogCompatibility(PpcDialogCompatibilityOperation),
    QuickDrawCompatibility(PpcQuickDrawCompatibilityOperation),
    SystemCompatibility(PpcSystemCompatibilityOperation),
    FileCompatibility(PpcFileCompatibilityOperation),
    AppleTalkCompatibility(PpcAppleTalkCompatibilityOperation),
    PrintingCompatibility(PpcPrintingCompatibilityOperation),
    SlotCompatibility,
    StandardFileCompatibility(PpcStandardFileOperation),
    SoundInputCompatibility(PpcSoundInputCompatibilityOperation),
    SpeechCompatibility(PpcSpeechCompatibilityOperation),
    QuickTimeCompatibility(PpcQuickTimeCompatibilityOperation),
    InputSprocketCompatibility(PpcInputSprocketCompatibilityOperation),
    MathCompatibility(PpcMathCompatibilityOperation),
    StdCCompatibility(PpcStdCCompatibilityOperation),
    ObjectSupportCompatibility,
    ObjectSupportInit,
    ObjectSupportInstallAccessor,
    ObjectSupportGetAccessor,
    ObjectSupportCallAccessor,
    ObjectSupportDisposeToken,
    ObjectSupportRemoveAccessor,
    ObjectSupportSetCallbacks,
    ObjectSupportResolve,
    GlmSetMode,
    GlmSetFunc,
    GlmMalloc,
    GlmCalloc,
    GlmRealloc,
    GlmFree,
    GlmPageFreeAll,
    GlmGetError,
    AglChoosePixelFormat,
    AglDescribePixelFormat,
    AglDestroyPixelFormat,
    AglGetError,
    AglCreateContext,
    AglDestroyContext,
    AglSetCurrentContext,
    AglGetCurrentContext,
    AglSetDrawable,
    AglGetDrawable,
    AglUpdateContext,
    AglSwapBuffers,
    ReturnError(i16),
    ReturnNoErr,
    ReturnOne,
    NoOpPreserve,
    UnresolvedWeak,
    Unsupported,
}

pub(crate) fn dispatcher_target_for_import(
    library_name: &str,
    symbol_name: &str,
) -> PpcImportDispatcherTarget {
    // CFM Carbon applications link CarbonLib in place of InterfaceLib for
    // supported Toolbox APIs. An exact symbol match uses the same PPC ABI.
    // Carbon Porting Guide (2002), pp. 42–43 and 53–54.
    let library_name = match (library_name, symbol_name) {
        // Carbon's Multimedia CFM namespace exposes the same movie toolbox
        // entry points as QuickTimeLib. Bind only the operations implemented
        // below so other weak imports retain their unresolved state.
        (
            "Apple;Carbon;Multimedia",
            "GetMovieBox"
            | "DisposeMovie"
            | "SetMovieBox"
            | "EnterMovies"
            | "SetMovieGWorld"
            | "StopMovie"
            | "GetMoviesError"
            | "MoviesTask"
            | "StartMovie"
            | "IsMovieDone",
        ) => "QuickTimeLib",
        // CarbonLib exports these C math symbols with the same PowerPC
        // floating-point ABI as MathLib. Inside Macintosh: PowerPC Numerics
        // (1994), pp. 6-10--6-11 (sqrt), 10-17--10-19 (pow),
        // 10-29--10-30 (cos).
        ("CarbonLib", "cos" | "pow" | "round" | "sqrt") => "MathLib",
        ("CarbonLib", _) => "InterfaceLib",
        // Director's CFM runtime exports these ISO C entry points with the
        // same PowerPC signatures as StdCLib. Leave its private symbols
        // unresolved until they have their own implementations.
        (
            "MacromediaRuntimeLib",
            "fclose"
            | "fopen"
            | "fread"
            | "fseek"
            | "ftell"
            | "getenv"
            | "memcmp"
            | "memcpy"
            | "memmove"
            | "memset"
            | "qsort"
            | "sprintf"
            | "sscanf"
            | "strcat"
            | "strchr"
            | "strcmp"
            | "strcpy"
            | "strlen"
            | "strncmp"
            | "strncpy"
            | "strpbrk"
            | "time"
            | "vsprintf",
        ) => "StdCLib",
        _ => library_name,
    };
    match (library_name, symbol_name) {
        ("OpenGLMemory", "glmSetMode") => PpcImportDispatcherTarget::GlmSetMode,
        ("OpenGLMemory", "glmSetFunc") => PpcImportDispatcherTarget::GlmSetFunc,
        ("OpenGLMemory", "glmMalloc") => PpcImportDispatcherTarget::GlmMalloc,
        ("OpenGLMemory", "glmCalloc") => PpcImportDispatcherTarget::GlmCalloc,
        ("OpenGLMemory", "glmRealloc") => PpcImportDispatcherTarget::GlmRealloc,
        ("OpenGLMemory", "glmFree") => PpcImportDispatcherTarget::GlmFree,
        ("OpenGLMemory", "glmPageFreeAll") => PpcImportDispatcherTarget::GlmPageFreeAll,
        ("OpenGLMemory", "glmGetError") => PpcImportDispatcherTarget::GlmGetError,
        ("InterfaceLib", "_MPIsFullyInitialized" | "MPProcessors") => {
            PpcImportDispatcherTarget::ReturnOne
        }
        ("InterfaceLib", "MPCreateSemaphore") => PpcImportDispatcherTarget::MpCreateSemaphore,
        ("InterfaceLib", "MPDeleteSemaphore") => PpcImportDispatcherTarget::MpDeleteSemaphore,
        ("InterfaceLib", "MPSignalSemaphore") => PpcImportDispatcherTarget::MpSignalSemaphore,
        ("InterfaceLib", "MPWaitOnSemaphore") => PpcImportDispatcherTarget::MpWaitOnSemaphore,
        ("ColMgrLib", "getCollectionMgrLibVersion") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Version)
        }
        ("ColMgrLib", "NewCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::New)
        }
        ("ColMgrLib", "DisposeCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Dispose)
        }
        ("ColMgrLib", "CloneCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Clone)
        }
        ("ColMgrLib", "CountCollectionOwners") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::CountOwners)
        }
        ("ColMgrLib", "CopyCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Copy)
        }
        ("ColMgrLib", "GetCollectionDefaultAttributes") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetDefaultAttributes)
        }
        ("ColMgrLib", "SetCollectionDefaultAttributes") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::SetDefaultAttributes)
        }
        ("ColMgrLib", "CountCollectionItems") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::CountItems)
        }
        ("ColMgrLib", "AddCollectionItem") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::AddItem)
        }
        ("ColMgrLib", "GetCollectionItem") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetItem)
        }
        ("ColMgrLib", "RemoveCollectionItem") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::RemoveItem)
        }
        ("ColMgrLib", "SetCollectionItemInfo") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::SetItemInfo)
        }
        ("ColMgrLib", "GetCollectionItemInfo") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetItemInfo)
        }
        ("ColMgrLib", "ReplaceIndexedCollectionItem") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::ReplaceIndexedItem)
        }
        ("ColMgrLib", "GetIndexedCollectionItem") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetIndexedItem)
        }
        ("ColMgrLib", "RemoveIndexedCollectionItem") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::RemoveIndexedItem)
        }
        ("ColMgrLib", "SetIndexedCollectionItemInfo") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::SetIndexedItemInfo)
        }
        ("ColMgrLib", "GetIndexedCollectionItemInfo") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetIndexedItemInfo)
        }
        ("ColMgrLib", "CollectionTagExists") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::TagExists)
        }
        ("ColMgrLib", "CountCollectionTags") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::CountTags)
        }
        ("ColMgrLib", "GetIndexedCollectionTag") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetIndexedTag)
        }
        ("ColMgrLib", "CountTaggedCollectionItems") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::CountTaggedItems)
        }
        ("ColMgrLib", "GetTaggedCollectionItem") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetTaggedItem)
        }
        ("ColMgrLib", "GetTaggedCollectionItemInfo") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetTaggedItemInfo)
        }
        ("ColMgrLib", "PurgeCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Purge)
        }
        ("ColMgrLib", "PurgeCollectionTag") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::PurgeTag)
        }
        ("ColMgrLib", "EmptyCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Empty)
        }
        ("ColMgrLib", "FlattenCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Flatten)
        }
        ("ColMgrLib", "FlattenPartialCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::FlattenPartial)
        }
        ("ColMgrLib", "UnflattenCollection") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::Unflatten)
        }
        ("ColMgrLib", "GetCollectionExceptionProc") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetExceptionProc)
        }
        ("ColMgrLib", "SetCollectionExceptionProc") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::SetExceptionProc)
        }
        ("ColMgrLib", "AddCollectionItemHdl") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::AddItemHandle)
        }
        ("ColMgrLib", "GetCollectionItemHdl") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetItemHandle)
        }
        ("ColMgrLib", "ReplaceIndexedCollectionItemHdl") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::ReplaceIndexedItemHandle)
        }
        ("ColMgrLib", "GetIndexedCollectionItemHdl") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::GetIndexedItemHandle)
        }
        ("ColMgrLib", "FlattenCollectionToHdl") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::FlattenToHandle)
        }
        ("ColMgrLib", "UnflattenCollectionFromHdl") => {
            PpcImportDispatcherTarget::Collection(PpcCollectionOperation::UnflattenFromHandle)
        }
        ("InterfaceLib" | "ProcessMgrSupport", "InstallExceptionHandler") => {
            PpcImportDispatcherTarget::InstallExceptionHandler
        }
        (library_name, "Q3Initialize") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Initialize
        }
        (library_name, "Q3Exit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Exit
        }
        (library_name, "Q3GetVersion") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GetVersion
        }
        (library_name, "Q3MemoryStorage_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MemoryStorageNew
        }
        (library_name, "Q3MemoryStorage_NewBuffer") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer
        }
        (library_name, "Q3FSSpecStorage_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FSSpecStorageNew
        }
        (library_name, "Q3File_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FileNew
        }
        (library_name, "Q3View_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewNew
        }
        (library_name, "Q3TriMesh_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TriMeshNew
        }
        (library_name, "Q3TriMesh_EmptyData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TriMeshEmptyData
        }
        (library_name, "Q3AttributeSet_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AttributeSetNew
        }
        (library_name, "Q3ViewAngleAspectCamera_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewAngleAspectCameraNew
        }
        (library_name, "Q3OrthographicCamera_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3OrthographicCameraNew
        }
        (library_name, "Q3ViewPlaneCamera_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewPlaneCameraNew
        }
        (library_name, "Q3AmbientLight_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AmbientLightNew
        }
        (library_name, "Q3DirectionalLight_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3DirectionalLightNew
        }
        (library_name, "Q3PointLight_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PointLightNew
        }
        (library_name, "Q3SpotLight_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightNew
        }
        (library_name, "Q3LightGroup_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightGroupNew
        }
        (library_name, "Q3DisplayGroup_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3DisplayGroupNew
        }
        (library_name, "Q3OrderedDisplayGroup_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3OrderedDisplayGroupNew
        }
        (library_name, "Q3BackfacingStyle_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3BackfacingStyleNew
        }
        (library_name, "Q3InterpolationStyle_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3InterpolationStyleNew
        }
        (library_name, "Q3FillStyle_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FillStyleNew
        }
        (library_name, "Q3OrientationStyle_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3OrientationStyleNew
        }
        (library_name, "Q3MipmapTexture_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MipmapTextureNew
        }
        (library_name, "Q3TextureShader_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TextureShaderNew
        }
        (library_name, "Q3LambertIllumination_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LambertIlluminationNew
        }
        (library_name, "Q3NULLIllumination_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3NullIlluminationNew
        }
        (library_name, "Q3PhongIllumination_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PhongIlluminationNew
        }
        (library_name, "Q3PixmapDrawContext_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PixmapDrawContextNew
        }
        (library_name, "Q3MacDrawContext_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MacDrawContextNew
        }
        (library_name, "Q3MatrixTransform_New") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MatrixTransformNew
        }
        (library_name, symbol_name)
            if is_quickdraw_3d_library(library_name)
                && symbol_name.starts_with("Q3")
                && symbol_name.ends_with("_New") =>
        {
            PpcImportDispatcherTarget::Q3NewObject
        }
        (library_name, "Q3Renderer_NewFromType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3RendererNewFromType
        }
        (library_name, "Q3Renderer_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3RendererGetType
        }
        (library_name, "Q3Renderer_Sync") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3RendererSync
        }
        (library_name, "Q3Renderer_Flush") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3RendererFlush
        }
        (library_name, "Q3InteractiveRenderer_SetDoubleBufferBypass")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3InteractiveRendererSetDoubleBufferBypass
        }
        (library_name, "Q3InteractiveRenderer_SetPreferences")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3InteractiveRendererSetPreferences
        }
        (library_name, "Q3InteractiveRenderer_SetRAVEContextHints")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveContextHints
        }
        (library_name, "Q3InteractiveRenderer_GetRAVEContextHints")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveContextHints
        }
        (library_name, "Q3InteractiveRenderer_GetRAVEDrawContexts")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveDrawContexts
        }
        (library_name, "Q3InteractiveRenderer_SetRAVETextureFilter")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveTextureFilter
        }
        (library_name, "Q3Object_Dispose") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ObjectDispose
        }
        (library_name, "Q3Object_Duplicate") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ObjectDuplicate
        }
        (library_name, "Q3Shared_GetReference") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SharedGetReference
        }
        (library_name, "Q3Shared_IsReferenced") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SharedIsReferenced
        }
        (library_name, "Q3Shared_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SharedGetType
        }
        (library_name, "Q3Shape_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShapeGetType
        }
        (library_name, "Q3Shape_GetLeafType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShapeGetLeafType
        }
        (library_name, "Q3Error_Get") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ErrorGet
        }
        (library_name, "Q3Object_IsDrawable") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ObjectIsDrawable
        }
        (library_name, "Q3Object_IsType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ObjectIsType
        }
        (library_name, "Q3Object_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ObjectGetType
        }
        (library_name, "Q3Object_GetLeafType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ObjectGetLeafType
        }
        (library_name, "Q3Light_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightGetType
        }
        (library_name, "Q3Geometry_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GeometryGetType
        }
        (library_name, "Q3Shader_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderGetType
        }
        (library_name, "Q3Group_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupGetType
        }
        (library_name, "Q3Matrix3x3_SetTranslate") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix3x3SetTranslate
        }
        (library_name, "Q3Matrix4x4_SetIdentity") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4SetIdentity
        }
        (library_name, "Q3Matrix4x4_SetTranslate") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4SetTranslate
        }
        (library_name, "Q3Matrix4x4_SetScale") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4SetScale
        }
        (library_name, "Q3Matrix4x4_SetRotate_X") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4SetRotateX
        }
        (library_name, "Q3Matrix4x4_SetRotate_Y") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4SetRotateY
        }
        (library_name, "Q3Matrix4x4_SetRotate_Z") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4SetRotateZ
        }
        (library_name, "Q3Matrix4x4_SetRotate_XYZ") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4SetRotateXyz
        }
        (library_name, "Q3Matrix4x4_Multiply") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4Multiply
        }
        (library_name, "Q3Matrix4x4_Transpose") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4Transpose
        }
        (library_name, "Q3Matrix4x4_Invert") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Matrix4x4Invert
        }
        (library_name, "Q3Point3D_Transform") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Point3DTransform
        }
        (library_name, "Q3Point3D_To3DTransformArray") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Point3DTo3DTransformArray
        }
        (library_name, "Q3Point3D_To4DTransformArray") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Point3DTo4DTransformArray
        }
        (library_name, "Q3Vector3D_Transform") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Vector3DTransform
        }
        (library_name, "Q3MatrixTransform_Set") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MatrixTransformSet
        }
        (library_name, "Q3Transform_GetMatrix") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TransformGetMatrix
        }
        (library_name, "Q3TextureShader_GetTexture") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TextureShaderGetTexture
        }
        (library_name, "Q3MipmapTexture_GetMipmap") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MipmapTextureGetMipmap
        }
        (library_name, "Q3Storage_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3StorageGetType
        }
        (library_name, "Q3DrawContext_GetPane") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3DrawContextGetPane
        }
        (library_name, "Q3MemoryStorage_Set") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MemoryStorageSet
        }
        (library_name, "Q3MemoryStorage_GetBuffer") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer
        }
        (library_name, "Q3MemoryStorage_SetBuffer") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MemoryStorageSetBuffer
        }
        (library_name, "Q3MemoryStorage_GetType") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MemoryStorageGetType
        }
        (library_name, "Q3Camera_GetPlacement") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraGetPlacement
        }
        (library_name, "Q3Camera_SetPlacement") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraSetPlacement
        }
        (library_name, "Q3Camera_GetRange") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraGetRange
        }
        (library_name, "Q3Camera_SetRange") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraSetRange
        }
        (library_name, "Q3Camera_GetViewPort") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraGetViewPort
        }
        (library_name, "Q3Camera_SetViewPort") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraSetViewPort
        }
        (library_name, "Q3Camera_GetWorldToView") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraGetWorldToView
        }
        (library_name, "Q3Camera_GetViewToFrustum") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3CameraGetViewToFrustum
        }
        (library_name, "Q3Light_GetState") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightGetState
        }
        (library_name, "Q3Light_SetState") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightSetState
        }
        (library_name, "Q3Light_GetBrightness") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightGetBrightness
        }
        (library_name, "Q3Light_SetBrightness") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightSetBrightness
        }
        (library_name, "Q3Light_GetColor") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightGetColor
        }
        (library_name, "Q3Light_SetColor") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightSetColor
        }
        (library_name, "Q3Light_GetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightGetData
        }
        (library_name, "Q3Light_SetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3LightSetData
        }
        (library_name, "Q3AmbientLight_GetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AmbientLightGetData
        }
        (library_name, "Q3AmbientLight_SetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AmbientLightSetData
        }
        (library_name, "Q3DirectionalLight_GetCastShadowsState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3DirectionalLightGetCastShadowsState
        }
        (library_name, "Q3DirectionalLight_SetCastShadowsState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3DirectionalLightSetCastShadowsState
        }
        (library_name, "Q3DirectionalLight_GetDirection")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3DirectionalLightGetDirection
        }
        (library_name, "Q3DirectionalLight_SetDirection")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3DirectionalLightSetDirection
        }
        (library_name, "Q3DirectionalLight_GetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3DirectionalLightGetData
        }
        (library_name, "Q3DirectionalLight_SetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3DirectionalLightSetData
        }
        (library_name, "Q3PointLight_GetCastShadowsState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3PointLightGetCastShadowsState
        }
        (library_name, "Q3PointLight_SetCastShadowsState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3PointLightSetCastShadowsState
        }
        (library_name, "Q3PointLight_GetAttenuation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PointLightGetAttenuation
        }
        (library_name, "Q3PointLight_SetAttenuation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PointLightSetAttenuation
        }
        (library_name, "Q3PointLight_GetLocation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PointLightGetLocation
        }
        (library_name, "Q3PointLight_SetLocation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PointLightSetLocation
        }
        (library_name, "Q3PointLight_GetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PointLightGetData
        }
        (library_name, "Q3PointLight_SetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PointLightSetData
        }
        (library_name, "Q3SpotLight_GetCastShadowsState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3SpotLightGetCastShadowsState
        }
        (library_name, "Q3SpotLight_SetCastShadowsState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3SpotLightSetCastShadowsState
        }
        (library_name, "Q3SpotLight_GetAttenuation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightGetAttenuation
        }
        (library_name, "Q3SpotLight_SetAttenuation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightSetAttenuation
        }
        (library_name, "Q3SpotLight_GetLocation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightGetLocation
        }
        (library_name, "Q3SpotLight_SetLocation") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightSetLocation
        }
        (library_name, "Q3SpotLight_GetDirection") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightGetDirection
        }
        (library_name, "Q3SpotLight_SetDirection") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightSetDirection
        }
        (library_name, "Q3SpotLight_GetHotAngle") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightGetHotAngle
        }
        (library_name, "Q3SpotLight_SetHotAngle") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightSetHotAngle
        }
        (library_name, "Q3SpotLight_GetOuterAngle") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightGetOuterAngle
        }
        (library_name, "Q3SpotLight_SetOuterAngle") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightSetOuterAngle
        }
        (library_name, "Q3SpotLight_GetFallOff") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightGetFallOff
        }
        (library_name, "Q3SpotLight_SetFallOff") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightSetFallOff
        }
        (library_name, "Q3SpotLight_GetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightGetData
        }
        (library_name, "Q3SpotLight_SetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3SpotLightSetData
        }
        (library_name, "Q3Shader_GetUVTransform") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderGetUVTransform
        }
        (library_name, "Q3Shader_SetUVTransform") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderSetUVTransform
        }
        (library_name, "Q3Shader_GetUBoundary") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderGetUBoundary
        }
        (library_name, "Q3Shader_SetUBoundary") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderSetUBoundary
        }
        (library_name, "Q3Shader_GetVBoundary") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderGetVBoundary
        }
        (library_name, "Q3Shader_SetVBoundary") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderSetVBoundary
        }
        (library_name, "Q3BackfacingStyle_Get") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3BackfacingStyleGet
        }
        (library_name, "Q3BackfacingStyle_Set") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3BackfacingStyleSet
        }
        (library_name, "Q3InterpolationStyle_Get") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3InterpolationStyleGet
        }
        (library_name, "Q3InterpolationStyle_Set") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3InterpolationStyleSet
        }
        (library_name, "Q3FillStyle_Get") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FillStyleGet
        }
        (library_name, "Q3FillStyle_Set") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FillStyleSet
        }
        (library_name, "Q3OrientationStyle_Get") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3OrientationStyleGet
        }
        (library_name, "Q3OrientationStyle_Set") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3OrientationStyleSet
        }
        (library_name, "Q3TriMesh_GetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TriMeshGetData
        }
        (library_name, "Q3TriMesh_SetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TriMeshSetData
        }
        (library_name, "Q3AttributeSet_Add") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AttributeSetAdd
        }
        (library_name, "Q3AttributeSet_Get") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AttributeSetGet
        }
        (library_name, "Q3AttributeSet_Clear") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AttributeSetClear
        }
        (library_name, "Q3AttributeSet_Contains") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3AttributeSetContains
        }
        (library_name, "Q3AttributeSet_GetNextAttributeType")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3AttributeSetGetNextAttributeType
        }
        (library_name, "Q3Storage_GetSize") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3StorageGetSize
        }
        (library_name, "Q3Storage_GetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3StorageGetData
        }
        (library_name, "Q3Storage_SetData") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3StorageSetData
        }
        (library_name, "Q3Vector3D_Normalize") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Vector3DNormalize
        }
        (library_name, "Q3Vector3D_Length") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Vector3DLength
        }
        (library_name, "Q3Vector2D_Normalize") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Vector2DNormalize
        }
        (library_name, "Q3Vector3D_Cross") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Vector3DCross
        }
        (library_name, "Q3Point2D_Distance") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Point2DDistance
        }
        (library_name, "Q3Point3D_Distance") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Point3DDistance
        }
        (library_name, "Q3Point3D_CrossProductTri") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3Point3DCrossProductTri
        }
        (library_name, "Q3BoundingBox_SetFromPoints3D")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3BoundingBoxSetFromPoints3D
        }
        (library_name, "Q3File_SetStorage") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FileSetStorage
        }
        (library_name, "Q3File_OpenRead") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FileOpenRead
        }
        (library_name, "Q3File_ReadObject") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FileReadObject
        }
        (library_name, "Q3File_IsEndOfFile") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FileIsEndOfFile
        }
        (library_name, "Q3File_Close") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FileClose
        }
        (library_name, "Q3Group_AddObject") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupAddObject
        }
        (library_name, "Q3Group_AddObjectBefore") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupAddObjectBefore
        }
        (library_name, "Q3Group_CountObjects") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupCountObjects
        }
        (library_name, "Q3Group_GetFirstPosition") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupGetFirstPosition
        }
        (library_name, "Q3Group_GetNextPosition") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupGetNextPosition
        }
        (library_name, "Q3Group_GetFirstPositionOfType")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType
        }
        (library_name, "Q3Group_GetPositionObject") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupGetPositionObject
        }
        (library_name, "Q3Group_RemovePosition") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3GroupRemovePosition
        }
        (library_name, "Q3View_SetRenderer") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewSetRenderer
        }
        (library_name, "Q3View_GetRenderer") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewGetRenderer
        }
        (library_name, "Q3View_SetLightGroup") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewSetLightGroup
        }
        (library_name, "Q3View_GetLightGroup") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewGetLightGroup
        }
        (library_name, "Q3View_SetDrawContext") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewSetDrawContext
        }
        (library_name, "Q3View_GetDrawContext") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewGetDrawContext
        }
        (library_name, "Q3View_SetCamera") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewSetCamera
        }
        (library_name, "Q3View_GetCamera") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewGetCamera
        }
        (library_name, "Q3View_GetWorldToFrustumMatrixState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3ViewGetWorldToFrustumMatrixState
        }
        (library_name, "Q3View_GetFrustumToWindowMatrixState")
            if is_quickdraw_3d_library(library_name) =>
        {
            PpcImportDispatcherTarget::Q3ViewGetFrustumToWindowMatrixState
        }
        (library_name, "Q3View_StartRendering") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewStartRendering
        }
        (library_name, "Q3View_EndRendering") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewEndRendering
        }
        (library_name, "Q3View_StartBoundingBox") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewStartBoundingBox
        }
        (library_name, "Q3View_EndBoundingBox") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewEndBoundingBox
        }
        (library_name, "Q3View_StartBoundingSphere") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewStartBoundingSphere
        }
        (library_name, "Q3View_EndBoundingSphere") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewEndBoundingSphere
        }
        (library_name, "Q3View_Cancel") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ViewCancel
        }
        (library_name, "Q3Shader_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ShaderSubmit
        }
        (library_name, "Q3Style_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3StyleSubmit
        }
        (library_name, "Q3BackfacingStyle_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3BackfacingStyleSubmit
        }
        (library_name, "Q3InterpolationStyle_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3InterpolationStyleSubmit
        }
        (library_name, "Q3FillStyle_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FillStyleSubmit
        }
        (library_name, "Q3OrientationStyle_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3OrientationStyleSubmit
        }
        (library_name, "Q3FogStyle_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3FogStyleSubmit
        }
        (library_name, "Q3TriMesh_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3TriMeshSubmit
        }
        (library_name, "Q3MatrixTransform_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3MatrixTransformSubmit
        }
        (library_name, "Q3ResetTransform_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ResetTransformSubmit
        }
        (library_name, "Q3Push_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PushSubmit
        }
        (library_name, "Q3Pop_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3PopSubmit
        }
        (library_name, "Q3Object_Submit") if is_quickdraw_3d_library(library_name) => {
            PpcImportDispatcherTarget::Q3ObjectSubmit
        }
        (library_name, symbol_name)
            if is_quickdraw_3d_library(library_name)
                && is_quickdraw_3d_status_success_import(symbol_name) =>
        {
            PpcImportDispatcherTarget::ReturnOne
        }
        (library_name, "QADeviceGetFirstEngine")
            if is_quickdraw_3d_accelerator_library(library_name) =>
        {
            PpcImportDispatcherTarget::QADeviceGetFirstEngine
        }
        (library_name, "QADeviceGetNextEngine")
            if is_quickdraw_3d_accelerator_library(library_name) =>
        {
            PpcImportDispatcherTarget::QADeviceGetNextEngine
        }
        (library_name, "QAEngineGestalt") if is_quickdraw_3d_accelerator_library(library_name) => {
            PpcImportDispatcherTarget::QAEngineGestalt
        }
        ("MathLib", "ceil") => PpcImportDispatcherTarget::MathCeil,
        ("MathLib", "sqrt") => PpcImportDispatcherTarget::MathSqrt,
        ("MathLib", "exp") => PpcImportDispatcherTarget::MathExp,
        ("MathLib", "fabs") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Fabs)
        }
        ("MathLib", "sin") => PpcImportDispatcherTarget::MathSin,
        ("MathLib", "cos") => PpcImportDispatcherTarget::MathCos,
        ("MathLib", "round") => PpcImportDispatcherTarget::MathRound,
        ("MathLib", "rint") => PpcImportDispatcherTarget::MathRint,
        ("MathLib", "asin") => PpcImportDispatcherTarget::MathAsin,
        ("MathLib", "tan") => PpcImportDispatcherTarget::MathTan,
        ("MathLib", "atan") => PpcImportDispatcherTarget::MathAtan,
        ("MathLib", "atan2") => PpcImportDispatcherTarget::MathAtan2,
        ("MathLib", "pow") => PpcImportDispatcherTarget::MathPow,
        ("MathLib", "fmod") => PpcImportDispatcherTarget::MathFmod,
        ("MathLib", "log") => PpcImportDispatcherTarget::MathLog,
        ("MathLib", "log10") => PpcImportDispatcherTarget::MathLog10,
        ("MathLib", "nan") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Nan)
        }
        ("MathLib", "dtox80") => PpcImportDispatcherTarget::MathDtox80,
        ("InterfaceLib", "dec2num") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Dec2Num)
        }
        ("Math64Lib", "LongDoubleToSInt64") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::LongDoubleToSInt64)
        }
        ("Math64Lib", "LongDoubleToUInt64") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::LongDoubleToUInt64)
        }
        ("Math64Lib", "S32Set") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S32Set),
        ("Math64Lib", "S64Absolute") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Absolute)
        }
        ("Math64Lib", "S64Add") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Add),
        ("Math64Lib", "S64And") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64And),
        ("Math64Lib", "S64BitwiseAnd") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64BitwiseAnd)
        }
        ("Math64Lib", "S64BitwiseEor") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64BitwiseEor)
        }
        ("Math64Lib", "S64BitwiseNot") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64BitwiseNot)
        }
        ("Math64Lib", "S64BitwiseOr") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64BitwiseOr)
        }
        ("Math64Lib", "S64Compare") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Compare)
        }
        ("Math64Lib", "S64Divide") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Divide)
        }
        ("Math64Lib", "S64Eor") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Eor),
        ("Math64Lib", "S64Max") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Max),
        ("Math64Lib", "S64Min") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Min),
        ("Math64Lib", "S64Multiply") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Multiply)
        }
        ("Math64Lib", "S64Negate") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Negate)
        }
        ("Math64Lib", "S64Not") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Not),
        ("Math64Lib", "S64Or") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Or),
        ("Math64Lib", "S64Set") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Set),
        ("Math64Lib", "S64SetU") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64SetU),
        ("Math64Lib", "S64ShiftLeft") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64ShiftLeft)
        }
        ("Math64Lib", "S64ShiftRight") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64ShiftRight)
        }
        ("Math64Lib", "S64Subtract") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::S64Subtract)
        }
        ("Math64Lib", "SInt64ToLongDouble") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::SInt64ToLongDouble)
        }
        ("Math64Lib", "SInt64ToUInt64") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::SInt64ToUInt64)
        }
        ("Math64Lib", "U32SetU") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U32SetU),
        ("Math64Lib", "U64Add") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Add),
        ("Math64Lib", "U64And") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64And),
        ("Math64Lib", "U64BitwiseAnd") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64BitwiseAnd)
        }
        ("Math64Lib", "U64BitwiseEor") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64BitwiseEor)
        }
        ("Math64Lib", "U64BitwiseNot") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64BitwiseNot)
        }
        ("Math64Lib", "U64BitwiseOr") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64BitwiseOr)
        }
        ("Math64Lib", "U64Compare") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Compare)
        }
        ("Math64Lib", "U64Divide") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Divide)
        }
        ("Math64Lib", "U64Eor") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Eor),
        ("Math64Lib", "U64Max") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Max),
        ("Math64Lib", "U64Multiply") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Multiply)
        }
        ("Math64Lib", "U64Not") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Not),
        ("Math64Lib", "U64Or") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Or),
        ("Math64Lib", "U64Set") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Set),
        ("Math64Lib", "U64SetU") => PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64SetU),
        ("Math64Lib", "U64ShiftLeft") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64ShiftLeft)
        }
        ("Math64Lib", "U64ShiftRight") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64ShiftRight)
        }
        ("Math64Lib", "U64Subtract") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::U64Subtract)
        }
        ("Math64Lib", "UInt64ToLongDouble") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::UInt64ToLongDouble)
        }
        ("Math64Lib", "UInt64ToSInt64") => {
            PpcImportDispatcherTarget::Math64(PpcMath64Operation::UInt64ToSInt64)
        }
        ("MathLib", "pi") => PpcImportDispatcherTarget::NoOpPreserve,
        // ISO/IEC 9899:1990 §4.6.1.1: setjmp returns zero when invoked
        // directly. The supported callers import __setjmp without longjmp,
        // so they do not require restoration of the saved environment.
        ("StdCLib", "__setjmp") => PpcImportDispatcherTarget::ReturnNoErr,
        // Classic Text Services Manager startup registers the application.
        // The emulated process has no external input method to initialize.
        ("InterfaceLib", "InitTSMAwareApplication")
        | ("InterfaceLib", "CloseTSMAwareApplication") => PpcImportDispatcherTarget::ReturnNoErr,
        ("InterfaceLib", "SetScriptManagerVariable") => PpcImportDispatcherTarget::ReturnNoErr,
        // No input method is active in the emulated process, so Text Services
        // Manager leaves each EventRecord for the application to handle.
        ("InterfaceLib", "TSMEvent") | ("InterfaceLib", "TSMMenuSelect") => {
            PpcImportDispatcherTarget::ReturnNoErr
        }
        // The default virtual keyboard uses the Roman script (script 0).
        ("InterfaceLib", "KeyScript") => PpcImportDispatcherTarget::NoOpPreserve,
        // ISO C atexit registers process-termination cleanup. Classic games
        // remain resident until the emulated process is torn down, at which
        // point Systemless releases all guest state together.
        ("StdCLib", "atexit") => PpcImportDispatcherTarget::ReturnNoErr,
        ("StdCLib", "clock") => PpcImportDispatcherTarget::TickCount,
        ("StdCLib", "memset") => PpcImportDispatcherTarget::StdMemset,
        ("StdCLib", "memcmp") => PpcImportDispatcherTarget::StdMemcmp,
        ("StdCLib", "memcpy") => PpcImportDispatcherTarget::StdMemcpy,
        ("StdCLib", "memmove") => PpcImportDispatcherTarget::StdMemmove,
        ("StdCLib", "malloc") => PpcImportDispatcherTarget::StdMalloc,
        ("StdCLib", "free") => PpcImportDispatcherTarget::StdFree,
        ("StdCLib", "calloc") => PpcImportDispatcherTarget::StdCalloc,
        ("StdCLib", "realloc") => PpcImportDispatcherTarget::StdRealloc,
        ("StdCLib", "strcpy") => PpcImportDispatcherTarget::StdStrcpy,
        ("StdCLib", "strcat") => PpcImportDispatcherTarget::StdStrcat,
        ("StdCLib", "strncpy") => PpcImportDispatcherTarget::StdStrncpy,
        ("StdCLib", "strncat") => PpcImportDispatcherTarget::StdStrncat,
        ("StdCLib", "strcmp") => PpcImportDispatcherTarget::StdStrcmp,
        ("StdCLib", "strncmp") => PpcImportDispatcherTarget::StdStrncmp,
        ("StdCLib", "strlen") => PpcImportDispatcherTarget::StdStrlen,
        // Apple Universal Interfaces 3.4 string.h declares these with the
        // standard C pointer, int, and size_t signatures for PowerPC CFM.
        ("StdCLib", "memchr") => PpcImportDispatcherTarget::StdMemchr,
        ("StdCLib", "strchr") => PpcImportDispatcherTarget::StdStrchr,
        ("StdCLib", "strrchr") => PpcImportDispatcherTarget::StdStrrchr,
        ("StdCLib", "strspn") => PpcImportDispatcherTarget::StdStrspn,
        ("StdCLib", "strcspn") => PpcImportDispatcherTarget::StdStrcspn,
        ("StdCLib", "strpbrk") => PpcImportDispatcherTarget::StdStrpbrk,
        ("StdCLib", "strstr") => PpcImportDispatcherTarget::StdStrstr,
        ("StdCLib", "atoi") => PpcImportDispatcherTarget::StdAtoi,
        ("StdCLib", "getenv") => PpcImportDispatcherTarget::StdGetenv,
        ("StdCLib", "sprintf") => PpcImportDispatcherTarget::StdSprintf,
        ("StdCLib", "clearerr") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::ClearErr)
        }
        ("StdCLib", "_filbuf") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileBuffer)
        }
        ("StdCLib", "fclose") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileClose)
        }
        ("StdCLib", "feof") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileEof)
        }
        ("StdCLib", "ferror") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileError)
        }
        ("StdCLib", "fflush") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileFlush)
        }
        ("StdCLib", "fopen") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileOpen)
        }
        ("StdCLib", "fprintf") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FilePrintf)
        }
        ("StdCLib", "fread") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileRead)
        }
        ("StdCLib", "fseek") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileSeek)
        }
        ("StdCLib", "ftell") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileTell)
        }
        ("StdCLib", "fwrite") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileWrite)
        }
        ("StdCLib", "_iob") => {
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::IoBuffer)
        }
        // ISO/IEC 9899:2011 7.22.6.1 declares abs(int) and labs(long int).
        // Both arguments and results use one 32-bit register in the classic PPC ABI.
        ("StdCLib", "abs" | "labs") => PpcImportDispatcherTarget::StdAbs,
        ("StdCLib", "toupper") => PpcImportDispatcherTarget::StdToupper,
        ("StdCLib", "tolower") => PpcImportDispatcherTarget::StdTolower,
        // Universal Interfaces 3.4 ctype.h declares these StdCLib exports.
        ("StdCLib", "isalnum") => PpcImportDispatcherTarget::StdIsalnum,
        ("StdCLib", "isalpha") => PpcImportDispatcherTarget::StdIsalpha,
        ("StdCLib", "isascii") => PpcImportDispatcherTarget::StdIsascii,
        ("StdCLib", "iscntrl") => PpcImportDispatcherTarget::StdIscntrl,
        ("StdCLib", "isdigit") => PpcImportDispatcherTarget::StdIsdigit,
        ("StdCLib", "isgraph") => PpcImportDispatcherTarget::StdIsgraph,
        ("StdCLib", "islower") => PpcImportDispatcherTarget::StdIslower,
        ("StdCLib", "isprint") => PpcImportDispatcherTarget::StdIsprint,
        ("StdCLib", "ispunct") => PpcImportDispatcherTarget::StdIspunct,
        ("StdCLib", "isspace") => PpcImportDispatcherTarget::StdIsspace,
        ("StdCLib", "isupper") => PpcImportDispatcherTarget::StdIsupper,
        ("StdCLib", "isxdigit") => PpcImportDispatcherTarget::StdIsxdigit,
        ("StdCLib", "toascii") => PpcImportDispatcherTarget::StdToascii,
        ("StdCLib", "srand") => PpcImportDispatcherTarget::StdSrand,
        ("StdCLib", "rand") => PpcImportDispatcherTarget::StdRand,
        ("StdCLib", "time") => PpcImportDispatcherTarget::StdTime,
        (
            "StdCLib",
            "_IntEnv" | "__C_phase" | "__p_CType" | "__target_for_exit" | "_exit_status",
        ) => PpcImportDispatcherTarget::NoOpPreserve,
        ("StdCLib", "exit") => PpcImportDispatcherTarget::ExitToShell,
        ("StdCLib", "_BreakPoint") | ("StdCLib", "__NubAt3") => {
            PpcImportDispatcherTarget::NoOpPreserve
        }
        ("InterfaceLib", "AEInstallEventHandler") => {
            PpcImportDispatcherTarget::AEInstallEventHandler
        }
        ("DrawSprocketLib", "DSpStartup") => PpcImportDispatcherTarget::DSpStartup,
        ("DrawSprocketLib", "DSpGetVersion") => PpcImportDispatcherTarget::DSpGetVersion,
        ("DrawSprocketLib", "DSpShutdown") => PpcImportDispatcherTarget::DSpShutdown,
        ("DrawSprocketLib", "DSpGetFirstContext") => PpcImportDispatcherTarget::DSpGetFirstContext,
        ("DrawSprocketLib", "DSpGetNextContext") => PpcImportDispatcherTarget::DSpGetNextContext,
        ("DrawSprocketLib", "DSpProcessEvent") => PpcImportDispatcherTarget::DSpProcessEvent,
        ("DrawSprocketLib", "DSpBlit_Fastest") => PpcImportDispatcherTarget::DSpBlitFastest,
        ("DrawSprocketLib", "DSpCanUserSelectContext") => {
            PpcImportDispatcherTarget::DSpCanUserSelectContext
        }
        ("DrawSprocketLib", "DSpGetMouse") => PpcImportDispatcherTarget::DSpGetMouse,
        ("DrawSprocketLib", "DSpFindContextFromPoint") => {
            PpcImportDispatcherTarget::DSpFindContextFromPoint
        }
        ("DrawSprocketLib", "DSpContext_GlobalToLocal") => {
            PpcImportDispatcherTarget::DSpContextGlobalToLocal
        }
        ("DrawSprocketLib", "DSpContext_LocalToGlobal") => {
            PpcImportDispatcherTarget::DSpContextLocalToGlobal
        }
        ("DrawSprocketLib", "DSpFindBestContext") => PpcImportDispatcherTarget::DSpFindBestContext,
        ("DrawSprocketLib", "DSpFindBestContextOnDisplayID") => {
            PpcImportDispatcherTarget::DSpFindBestContextOnDisplayID
        }
        ("DrawSprocketLib", "DSpUserSelectContext") => {
            PpcImportDispatcherTarget::DSpUserSelectContext
        }
        ("DrawSprocketLib", "DSpSetBlankingColor") => {
            PpcImportDispatcherTarget::DSpSetBlankingColor
        }
        ("DrawSprocketLib", "DSpAltBuffer_New") => PpcImportDispatcherTarget::DSpAltBufferNew,
        ("DrawSprocketLib", "DSpAltBuffer_GetCGrafPtr") => {
            PpcImportDispatcherTarget::DSpAltBufferGetCGrafPtr
        }
        ("DrawSprocketLib", "DSpContext_Reserve") => PpcImportDispatcherTarget::DSpContextReserve,
        ("DrawSprocketLib", "DSpContext_Release") => PpcImportDispatcherTarget::DSpContextRelease,
        ("DrawSprocketLib", "DSpContext_SetState") => PpcImportDispatcherTarget::DSpContextSetState,
        ("DrawSprocketLib", "DSpContext_GetState") => PpcImportDispatcherTarget::DSpContextGetState,
        ("DrawSprocketLib", "DSpContext_FadeGamma") => {
            PpcImportDispatcherTarget::DSpContextFadeGamma
        }
        ("DrawSprocketLib", "DSpContext_FadeGammaIn") => {
            PpcImportDispatcherTarget::DSpContextFadeGammaIn
        }
        ("DrawSprocketLib", "DSpContext_FadeGammaOut") => {
            PpcImportDispatcherTarget::DSpContextFadeGammaOut
        }
        ("DrawSprocketLib", "DSpContext_GetFrontBuffer") => {
            PpcImportDispatcherTarget::DSpContextGetFrontBuffer
        }
        ("DrawSprocketLib", "DSpContext_GetBackBuffer") => {
            PpcImportDispatcherTarget::DSpContextGetBackBuffer
        }
        ("DrawSprocketLib", "DSpContext_SwapBuffers") => {
            PpcImportDispatcherTarget::DSpContextSwapBuffers
        }
        ("DrawSprocketLib", "DSpContext_SetCLUTEntries") => {
            PpcImportDispatcherTarget::DSpContextSetClutEntries
        }
        ("DrawSprocketLib", "DSpContext_GetCLUTEntries") => {
            PpcImportDispatcherTarget::DSpContextGetClutEntries
        }
        ("DrawSprocketLib", "DSpContext_GetDisplayID") => {
            PpcImportDispatcherTarget::DSpContextGetDisplayID
        }
        ("DrawSprocketLib", "DSpContext_GetAttributes") => {
            PpcImportDispatcherTarget::DSpContextGetAttributes
        }
        ("DrawSprocketLib", "DSpContext_GetFlattenedSize") => {
            PpcImportDispatcherTarget::DSpContextGetFlattenedSize
        }
        ("DrawSprocketLib", "DSpContext_Flatten") => PpcImportDispatcherTarget::DSpContextFlatten,
        ("DrawSprocketLib", "DSpContext_Restore") => PpcImportDispatcherTarget::DSpContextRestore,
        ("DrawSprocketLib", "DSpContext_SetVBLProc") => {
            PpcImportDispatcherTarget::DSpContextSetVblProc
        }
        ("DrawSprocketLib", "DSpContext_IsBusy") => PpcImportDispatcherTarget::DSpContextIsBusy,
        ("DrawSprocketLib", "DSpAltBuffer_Dispose") => {
            PpcImportDispatcherTarget::DSpAltBufferDispose
        }
        ("DrawSprocketLib", "DSpContext_InvalBackBufferRect") => {
            PpcImportDispatcherTarget::DSpContextInvalBackBufferRect
        }
        ("DrawSprocketLib", "DSpContext_SetUnderlayAltBuffer") => {
            PpcImportDispatcherTarget::DSpContextSetUnderlayAltBuffer
        }
        ("InputSprocketLib", "ISpElement_NewVirtualFromNeeds") => {
            PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds
        }
        ("InputSprocketLib", "ISpElementList_New") => PpcImportDispatcherTarget::ISpElementListNew,
        ("InputSprocketLib", "ISpElementList_AddElements") => {
            PpcImportDispatcherTarget::ISpElementListAddElements
        }
        ("InputSprocketLib", "ISpElementList_GetNextEvent") => {
            PpcImportDispatcherTarget::ISpElementListGetNextEvent
        }
        ("InputSprocketLib", "ISpElementList_Flush") => {
            PpcImportDispatcherTarget::ISpElementListFlush
        }
        ("InputSprocketLib", "ISpDevices_Extract") => PpcImportDispatcherTarget::ISpDevicesExtract,
        ("InputSprocketLib", "ISpDevices_ExtractByClass") => {
            PpcImportDispatcherTarget::ISpDevicesExtractByClass
        }
        ("InputSprocketLib", "ISpDevice_GetDefinition") => {
            PpcImportDispatcherTarget::ISpDeviceGetDefinition
        }
        ("InputSprocketLib", "ISpDevice_GetElementList") => {
            PpcImportDispatcherTarget::ISpDeviceGetElementList
        }
        ("InputSprocketLib", "ISpElementList_Extract") => {
            PpcImportDispatcherTarget::ISpElementListExtract
        }
        ("InputSprocketLib", "ISpElement_GetInfo") => PpcImportDispatcherTarget::ISpElementGetInfo,
        ("InputSprocketLib", "ISpElement_GetConfigurationInfo") => {
            PpcImportDispatcherTarget::ISpElementGetConfigurationInfo
        }
        ("InputSprocketLib", "ISpElement_GetSimpleState") => {
            PpcImportDispatcherTarget::ISpElementGetSimpleState
        }
        ("InputSprocketLib", "ISpGetVersion") => PpcImportDispatcherTarget::ISpGetVersion,
        ("InputSprocketLib", "ISpStartup") => PpcImportDispatcherTarget::ISpStartup,
        ("InputSprocketLib", "ISpShutdown") => PpcImportDispatcherTarget::ISpShutdown,
        ("InputSprocketLib", "ISpInit") => PpcImportDispatcherTarget::ISpInit,
        ("InputSprocketLib", "ISpStop") => PpcImportDispatcherTarget::ISpStop,
        ("InputSprocketLib", "ISpSuspend") => PpcImportDispatcherTarget::ISpSuspend,
        ("InputSprocketLib", "ISpResume") => PpcImportDispatcherTarget::ISpResume,
        ("InputSprocketLib", "ISpDevices_Activate") => {
            PpcImportDispatcherTarget::ISpDevicesActivate
        }
        ("InputSprocketLib", "ISpDevices_Deactivate") => {
            PpcImportDispatcherTarget::ISpDevicesDeactivate
        }
        ("InputSprocketLib", "ISpConfigure") => PpcImportDispatcherTarget::ISpConfigure,
        ("3DfxGlideLib2.x", "grSstQueryBoards") => PpcImportDispatcherTarget::GlideSstQueryBoards,
        ("InterfaceLib", "NewPtr") | ("InterfaceLib", "NewPtrSys") => {
            PpcImportDispatcherTarget::NewPtr { clear: false }
        }
        ("InterfaceLib", "NewPtrClear") | ("InterfaceLib", "NewPtrSysClear") => {
            PpcImportDispatcherTarget::NewPtr { clear: true }
        }
        ("InterfaceLib", "DisposePtr") => PpcImportDispatcherTarget::DisposePtr,
        ("InterfaceLib", "GetPtrSize") => PpcImportDispatcherTarget::GetPtrSize,
        ("InterfaceLib", "SetPtrSize") => PpcImportDispatcherTarget::SetPtrSize,
        ("InterfaceLib", "RecoverHandle") => PpcImportDispatcherTarget::RecoverHandle,
        ("InterfaceLib", "NewHandle") => PpcImportDispatcherTarget::NewHandle { clear: false },
        ("InterfaceLib", "NewHandleClear") => PpcImportDispatcherTarget::NewHandle { clear: true },
        ("InterfaceLib", "TempNewHandle") => PpcImportDispatcherTarget::TempNewHandle,
        ("InterfaceLib", "TempDisposeHandle") => PpcImportDispatcherTarget::TempDisposeHandle,
        ("InterfaceLib", "HoldMemory") => PpcImportDispatcherTarget::HoldMemory,
        ("InterfaceLib", "UnholdMemory") => PpcImportDispatcherTarget::UnholdMemory,
        ("InterfaceLib", "DisposeHandle") => PpcImportDispatcherTarget::DisposeHandle,
        ("InterfaceLib", "EmptyHandle") => PpcImportDispatcherTarget::EmptyHandle,
        ("InterfaceLib", "BlockMove") | ("InterfaceLib", "BlockMoveData") => {
            PpcImportDispatcherTarget::BlockMove
        }
        ("InterfaceLib", "BlockZero") => PpcImportDispatcherTarget::BlockZero,
        ("InterfaceLib", "PtrToHand") => PpcImportDispatcherTarget::PtrToHand,
        ("InterfaceLib", "PtrToXHand") => PpcImportDispatcherTarget::PtrToXHand,
        ("InterfaceLib", "HandToHand") => PpcImportDispatcherTarget::HandToHand,
        ("InterfaceLib", "HandAndHand") => PpcImportDispatcherTarget::HandAndHand,
        ("InterfaceLib", "GetHandleSize") => PpcImportDispatcherTarget::GetHandleSize,
        ("InterfaceLib", "SetHandleSize") => PpcImportDispatcherTarget::SetHandleSize,
        ("InterfaceLib", "HLock") => PpcImportDispatcherTarget::HLock,
        ("InterfaceLib", "HLockHi") => PpcImportDispatcherTarget::HLockHi,
        ("InterfaceLib", "HGetState") => PpcImportDispatcherTarget::HGetState,
        ("InterfaceLib", "HSetState") => PpcImportDispatcherTarget::HSetState,
        ("InterfaceLib", "HUnlock") => PpcImportDispatcherTarget::HUnlock,
        ("InterfaceLib", "MoveHHi") => PpcImportDispatcherTarget::MoveHHi,
        ("InterfaceLib", "HNoPurge") => PpcImportDispatcherTarget::HNoPurge,
        ("InterfaceLib", "HPurge") => PpcImportDispatcherTarget::HPurge,
        ("InterfaceLib", "TickCount") => PpcImportDispatcherTarget::TickCount,
        ("InterfaceLib", "GetZone") => PpcImportDispatcherTarget::GetZone,
        ("InterfaceLib", "SetZone") => PpcImportDispatcherTarget::SetZone,
        ("InterfaceLib", "InitZone") => PpcImportDispatcherTarget::InitZone,
        ("InterfaceLib", "SystemZone") => PpcImportDispatcherTarget::SystemZone,
        ("InterfaceLib", "ApplicZone") | ("InterfaceLib", "ApplicationZone") => {
            PpcImportDispatcherTarget::ApplicationZone
        }
        ("InterfaceLib", "MaxApplZone") => PpcImportDispatcherTarget::MaxApplZone,
        ("InterfaceLib", "MakeDataExecutable")
        | ("InterfaceLib", "FlushCodeCache")
        | ("InterfaceLib", "FlushCodeCacheRange")
        | ("InterfaceLib", "FlushInstructionCache") => PpcImportDispatcherTarget::FlushCodeCache,
        ("InterfaceLib", "MoreMasters")
        | ("InterfaceLib", "MoreMasterPointers") => PpcImportDispatcherTarget::MoreMasters,
        ("InterfaceLib", "GetApplLimit") => PpcImportDispatcherTarget::GetApplLimit,
        ("InterfaceLib", "SetApplLimit") => PpcImportDispatcherTarget::SetApplLimit,
        ("InterfaceLib", "CompactMem")
        | ("InterfaceLib", "CompactMemSys")
        | ("InterfaceLib", "FreeMem")
        | ("InterfaceLib", "FreeMemSys") => PpcImportDispatcherTarget::HeapFreeBytes,
        ("InterfaceLib", "MaxMem") => PpcImportDispatcherTarget::MaxMem,
        ("InterfaceLib", "MemError") => PpcImportDispatcherTarget::MemError,
        ("InterfaceLib", "CurResFile") => PpcImportDispatcherTarget::CurResFile,
        ("InterfaceLib", "UseResFile") => PpcImportDispatcherTarget::UseResFile,
        ("InterfaceLib", "CloseResFile") => PpcImportDispatcherTarget::CloseResFile,
        ("InterfaceLib", "OpenResFile") => PpcImportDispatcherTarget::OpenResFile,
        ("InterfaceLib", "HOpenResFile") => PpcImportDispatcherTarget::HOpenResFile,
        ("InterfaceLib", "ResError") => PpcImportDispatcherTarget::ResError,
        ("InterfaceLib", "SetResLoad") => PpcImportDispatcherTarget::SetResLoad,
        ("InterfaceLib", "LMGetResLoad") => PpcImportDispatcherTarget::LMGetResLoad,
        ("InterfaceLib", "LoadResource") => PpcImportDispatcherTarget::LoadResource,
        ("InterfaceLib", "GetIndString") | ("InterfaceLib", "getindstring") => {
            PpcImportDispatcherTarget::GetIndString
        }
        ("InterfaceLib", "GetString") => PpcImportDispatcherTarget::GetString,
        ("InterfaceLib", "GetResource") => PpcImportDispatcherTarget::GetResource,
        ("InterfaceLib", "Get1Resource") => PpcImportDispatcherTarget::Get1Resource,
        ("InterfaceLib", "GetNamedResource") => PpcImportDispatcherTarget::GetNamedResource,
        ("InterfaceLib", "Get1NamedResource") => PpcImportDispatcherTarget::Get1NamedResource,
        ("InterfaceLib", "GetIndResource") => PpcImportDispatcherTarget::GetIndResource,
        ("InterfaceLib", "Get1IndResource") => PpcImportDispatcherTarget::Get1IndResource,
        ("InterfaceLib", "GetResAttrs") => PpcImportDispatcherTarget::GetResAttrs,
        ("InterfaceLib", "SetResAttrs") => PpcImportDispatcherTarget::SetResAttrs,
        ("InterfaceLib", "GetResInfo") => PpcImportDispatcherTarget::GetResInfo,
        ("InterfaceLib", "GetResourceSizeOnDisk")
        | ("InterfaceLib", "GetMaxResourceSize")
        | ("InterfaceLib", "MaxSizeRsrc")
        | ("InterfaceLib", "SizeResource") => PpcImportDispatcherTarget::GetResourceSizeOnDisk,
        ("InterfaceLib", "SetResInfo") => PpcImportDispatcherTarget::SetResInfo,
        ("InterfaceLib", "HomeResFile") => PpcImportDispatcherTarget::HomeResFile,
        ("InterfaceLib", "CountResources") => PpcImportDispatcherTarget::CountResources,
        ("InterfaceLib", "Count1Resources") => PpcImportDispatcherTarget::Count1Resources,
        ("InterfaceLib", "CountTypes") => PpcImportDispatcherTarget::CountTypes,
        ("InterfaceLib", "Count1Types") => PpcImportDispatcherTarget::Count1Types,
        ("InterfaceLib", "GetIndType") => PpcImportDispatcherTarget::GetIndType,
        ("InterfaceLib", "Get1IndType") => PpcImportDispatcherTarget::Get1IndType,
        ("InterfaceLib", "UniqueID") => PpcImportDispatcherTarget::UniqueID,
        ("InterfaceLib", "Unique1ID") => PpcImportDispatcherTarget::Unique1ID,
        ("InterfaceLib", "UpdateResFile") => PpcImportDispatcherTarget::UpdateResFile,
        ("InterfaceLib", "AddResource") => PpcImportDispatcherTarget::AddResource,
        ("InterfaceLib", "ChangedResource") => PpcImportDispatcherTarget::ChangedResource,
        ("InterfaceLib", "WriteResource") => PpcImportDispatcherTarget::WriteResource,
        ("InterfaceLib", "RemoveResource") => PpcImportDispatcherTarget::RemoveResource,
        ("InterfaceLib", "GetPicture") => PpcImportDispatcherTarget::GetPicture,
        ("InterfaceLib", "GetIndPattern") => PpcImportDispatcherTarget::GetIndPattern,
        ("InterfaceLib", "GetPixPat") => PpcImportDispatcherTarget::GetPixPat,
        ("InterfaceLib", "GetPictInfo") => PpcImportDispatcherTarget::GetPictInfo,
        ("InterfaceLib", "DrawPicture") => PpcImportDispatcherTarget::DrawPicture,
        ("InterfaceLib", "KillPicture") => PpcImportDispatcherTarget::KillPicture,
        ("InterfaceLib", "Gestalt") => PpcImportDispatcherTarget::Gestalt,
        ("InterfaceLib", "NewGestaltValue") => PpcImportDispatcherTarget::NewGestaltValue,
        // Universal Interfaces exposes Code Fragment Manager entry points
        // through several compatibility libraries across classic and Carbon
        // runtimes. Treat the library name as an export namespace alias; the
        // calling convention and API contract are identical.
        (
            "InterfaceLib" | "CodeFragmentMgr" | "CarbonCore.vlib" | "CFMPriv_CarbonCore",
            "GetSharedLibrary",
        ) => PpcImportDispatcherTarget::GetSharedLibrary,
        (
            "InterfaceLib" | "CodeFragmentMgr" | "CarbonCore.vlib" | "CFMPriv_CarbonCore",
            "FindSymbol",
        ) => PpcImportDispatcherTarget::FindSymbol,
        (
            "InterfaceLib" | "CodeFragmentMgr" | "CarbonCore.vlib" | "CFMPriv_CarbonCore",
            "CountSymbols",
        ) => PpcImportDispatcherTarget::CountSymbols,
        (
            "InterfaceLib" | "CodeFragmentMgr" | "CarbonCore.vlib" | "CFMPriv_CarbonCore",
            "GetIndSymbol",
        ) => PpcImportDispatcherTarget::GetIndSymbol,
        (
            "InterfaceLib" | "CodeFragmentMgr" | "CarbonCore.vlib" | "CFMPriv_CarbonCore",
            "CloseConnection",
        ) => PpcImportDispatcherTarget::CloseConnection,
        ("InterfaceLib", "GetMemFragment") => PpcImportDispatcherTarget::GetMemFragment,
        ("InterfaceLib", "GetDiskFragment") => PpcImportDispatcherTarget::GetDiskFragment,
        ("InterfaceLib", "InitCursor") => PpcImportDispatcherTarget::InitCursor,
        ("InterfaceLib", "GetQDGlobalsArrow") => PpcImportDispatcherTarget::GetQDGlobalsArrow,
        ("InterfaceLib", "HideCursor") => PpcImportDispatcherTarget::HideCursor,
        ("InterfaceLib", "ShowCursor") => PpcImportDispatcherTarget::ShowCursor,
        ("InterfaceLib", "ShieldCursor") => PpcImportDispatcherTarget::ShieldCursor,
        ("InterfaceLib", "CrsrDevNextDevice") => PpcImportDispatcherTarget::CrsrDevNextDevice,
        ("InterfaceLib", "CrsrDevMoveTo") => PpcImportDispatcherTarget::CrsrDevMoveTo,
        ("InterfaceLib", "GetCursor") => PpcImportDispatcherTarget::GetCursor,
        ("InterfaceLib", "SetCursor") => PpcImportDispatcherTarget::SetCursor,
        ("InterfaceLib", "GetCCursor") => PpcImportDispatcherTarget::GetCCursor,
        ("InterfaceLib", "GetCIcon") => PpcImportDispatcherTarget::GetCIcon,
        ("InterfaceLib", "PlotCIcon") => PpcImportDispatcherTarget::PlotCIcon,
        ("InterfaceLib", "DisposeCIcon") => PpcImportDispatcherTarget::DisposeCIcon,
        ("InterfaceLib", "SetCCursor") => PpcImportDispatcherTarget::SetCCursor,
        ("InterfaceLib", "DisposeCCursor") => PpcImportDispatcherTarget::DisposeCCursor,
        ("InterfaceLib", "SysBeep") => PpcImportDispatcherTarget::SysBeep,
        ("InterfaceLib", "GetForeColor") => PpcImportDispatcherTarget::GetForeColor,
        ("InterfaceLib", "GetBackColor") => PpcImportDispatcherTarget::GetBackColor,
        ("InterfaceLib", "ForeColor") => PpcImportDispatcherTarget::ForeColor,
        ("InterfaceLib", "BackColor") => PpcImportDispatcherTarget::BackColor,
        ("InterfaceLib", "RGBForeColor") => PpcImportDispatcherTarget::RGBForeColor,
        ("InterfaceLib", "RGBBackColor") => PpcImportDispatcherTarget::RGBBackColor,
        ("InterfaceLib", "OpColor") => PpcImportDispatcherTarget::OpColor,
        ("InterfaceLib", "HiliteColor") => PpcImportDispatcherTarget::HiliteColor,
        ("InterfaceLib", "PmForeColor") => PpcImportDispatcherTarget::PmForeColor,
        ("InterfaceLib", "PmBackColor") => PpcImportDispatcherTarget::PmBackColor,
        ("InterfaceLib", "Color2Index") => PpcImportDispatcherTarget::Color2Index,
        ("InterfaceLib", "Index2Color") => PpcImportDispatcherTarget::Index2Color,
        ("InterfaceLib", "RGB2HSL") => PpcImportDispatcherTarget::RGB2HSL,
        ("InterfaceLib", "RGB2HSV") => PpcImportDispatcherTarget::RGB2HSV,
        ("InterfaceLib", "HSV2RGB") => PpcImportDispatcherTarget::HSV2RGB,
        ("InterfaceLib", "FixRatio") => PpcImportDispatcherTarget::FixRatio,
        ("InterfaceLib", "FixMul") => PpcImportDispatcherTarget::FixMul,
        ("InterfaceLib", "FixDiv") => PpcImportDispatcherTarget::FixDiv,
        ("InterfaceLib", "Long2Fix") => PpcImportDispatcherTarget::Long2Fix,
        ("InterfaceLib", "Fix2Long") => PpcImportDispatcherTarget::Fix2Long,
        ("InterfaceLib", "FixRound") => PpcImportDispatcherTarget::FixRound,
        ("InterfaceLib", "Fix2Frac") => PpcImportDispatcherTarget::Fix2Frac,
        ("InterfaceLib", "Frac2Fix") => PpcImportDispatcherTarget::Frac2Fix,
        ("InterfaceLib", "Frac2X") => PpcImportDispatcherTarget::Frac2X,
        ("InterfaceLib", "X2Frac") => PpcImportDispatcherTarget::X2Frac,
        ("InterfaceLib", "FracSin") => PpcImportDispatcherTarget::FracSin,
        ("InterfaceLib", "FracCos") => PpcImportDispatcherTarget::FracCos,
        ("InterfaceLib", "FracSqrt") => PpcImportDispatcherTarget::FracSqrt,
        ("InterfaceLib", "FracMul") => PpcImportDispatcherTarget::FracMul,
        ("InterfaceLib", "FracDiv") => PpcImportDispatcherTarget::FracDiv,
        ("InterfaceLib", "FixATan2") => PpcImportDispatcherTarget::FixATan2,
        ("InterfaceLib", "WideAdd") => PpcImportDispatcherTarget::WideAdd,
        ("InterfaceLib", "WideSubtract") => PpcImportDispatcherTarget::WideSubtract,
        ("InterfaceLib", "WideNegate") => PpcImportDispatcherTarget::WideNegate,
        ("InterfaceLib", "WideShift") => PpcImportDispatcherTarget::WideShift,
        ("InterfaceLib", "WideBitShift") => PpcImportDispatcherTarget::WideBitShift,
        ("InterfaceLib", "WideMultiply") => PpcImportDispatcherTarget::WideMultiply,
        ("InterfaceLib", "WideDivide") => PpcImportDispatcherTarget::WideDivide,
        ("InterfaceLib", "WideWideDivide") => PpcImportDispatcherTarget::WideWideDivide,
        ("InterfaceLib", "WideSquareRoot") => PpcImportDispatcherTarget::WideSquareRoot,
        ("InterfaceLib", "WideCompare") => PpcImportDispatcherTarget::WideCompare,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "NewMenu" | "newmenu",
        ) => PpcImportDispatcherTarget::NewMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DisposeMenu" | "disposemenu" | "DisposMenu" | "disposmenu",
        ) => PpcImportDispatcherTarget::DisposeMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetMenu" | "getmenu",
        ) => PpcImportDispatcherTarget::GetMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetItemCmd" | "getitemcmd" | "GetMenuItemCmd" | "getmenuitemcmd",
        ) => PpcImportDispatcherTarget::GetItemCmd,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetItemCmd" | "setitemcmd" | "SetMenuItemCmd" | "setmenuitemcmd",
        ) => PpcImportDispatcherTarget::SetItemCmd,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetItemMark" | "getitemmark" | "GetMenuItemMark" | "getmenuitemmark",
        ) => PpcImportDispatcherTarget::GetItemMark,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CountMItems" | "countmitems" | "CountMenuItems" | "countmenuitems",
        ) => PpcImportDispatcherTarget::CountMItems,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetMenuItemText" | "getmenuitemtext" | "GetItem" | "getitem",
        ) => PpcImportDispatcherTarget::GetMenuItemText,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetMenuItemText" | "setmenuitemtext" | "SetItem" | "setitem",
        ) => PpcImportDispatcherTarget::SetMenuItemText,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DeleteMenuItem" | "deletemenuitem" | "DelMenuItem" | "delmenuitem",
        ) => PpcImportDispatcherTarget::DeleteMenuItem,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CalcMenuSize" | "calcmenusize",
        ) => PpcImportDispatcherTarget::CalcMenuSize,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "PopUpMenuSelect" | "popupmenuselect",
        ) => PpcImportDispatcherTarget::PopUpMenuSelect,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InsertMenu" | "insertmenu",
        ) => PpcImportDispatcherTarget::InsertMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DeleteMenu" | "deletemenu" | "DelMenu" | "delmenu",
        ) => PpcImportDispatcherTarget::DeleteMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "AppendMenu" | "appendmenu",
        ) => PpcImportDispatcherTarget::AppendMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InsertMenuItem" | "insertmenuitem" | "InsMenuItem" | "insmenuitem",
        ) => PpcImportDispatcherTarget::InsertMenuItem,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "AppendResMenu" | "appendresmenu" | "AddResMenu" | "addresmenu",
        ) => PpcImportDispatcherTarget::AppendResMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InsertResMenu" | "insertresmenu",
        ) => PpcImportDispatcherTarget::InsertResMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "EnableItem" | "enableitem" | "EnableMenuItem" | "enablemenuitem",
        ) => PpcImportDispatcherTarget::EnableMenuItem,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DisableItem" | "disableitem" | "DisableMenuItem" | "disablemenuitem",
        ) => PpcImportDispatcherTarget::DisableMenuItem,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetItemMark" | "setitemmark" | "SetMenuItemMark" | "setmenuitemmark",
        ) => PpcImportDispatcherTarget::SetItemMark,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CheckItem" | "checkitem" | "CheckMenuItem" | "checkmenuitem",
        ) => PpcImportDispatcherTarget::CheckItem,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetMenuBar" | "getmenubar",
        ) => PpcImportDispatcherTarget::GetMenuBar,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetNewMBar" | "getnewmbar",
        ) => PpcImportDispatcherTarget::GetNewMBar,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMGetMenuList" | "lmgetmenulist",
        ) => PpcImportDispatcherTarget::LMGetMenuList,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMSetMenuHook" | "lmsetmenuhook",
        ) => PpcImportDispatcherTarget::LMSetMenuHook,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMGetMenuFlash" | "lmgetmenuflash",
        ) => PpcImportDispatcherTarget::LMGetMenuFlash,
        ("InterfaceLib", "LMGetPaintWhite") => PpcImportDispatcherTarget::LMGetPaintWhite,
        ("InterfaceLib", "LMGetSysMap") => PpcImportDispatcherTarget::LMGetSysMap,
        ("InterfaceLib", "LMGetCurApRefNum") => PpcImportDispatcherTarget::LMGetCurApRefNum,
        ("InterfaceLib", "GetVCBQHdr") => PpcImportDispatcherTarget::GetVCBQHdr,
        ("InterfaceLib", "GetDrvQHdr") => PpcImportDispatcherTarget::GetDrvQHdr,
        ("InterfaceLib", "LMGetSysEvtMask") => PpcImportDispatcherTarget::LMGetSysEvtMask,
        ("InterfaceLib", "LMSetSysEvtMask") => PpcImportDispatcherTarget::LMSetSysEvtMask,
        ("InterfaceLib", "LMGetDefltStack") => PpcImportDispatcherTarget::LMGetDefltStack,
        ("InterfaceLib", "LMGetCurStackBase") => PpcImportDispatcherTarget::LMGetCurStackBase,
        ("InterfaceLib", "LMSetPaintWhite") => PpcImportDispatcherTarget::LMSetPaintWhite,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMSetResumeProc" | "lmsetresumeproc",
        ) => PpcImportDispatcherTarget::LMSetResumeProc,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMGetResumeProc" | "lmgetresumeproc",
        ) => PpcImportDispatcherTarget::LMGetResumeProc,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMSetACount" | "lmsetacount",
        ) => PpcImportDispatcherTarget::LMSetACount,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMGetACount" | "lmgetacount",
        ) => PpcImportDispatcherTarget::LMGetACount,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMSetANumber" | "lmsetanumber",
        ) => PpcImportDispatcherTarget::LMSetANumber,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMGetANumber" | "lmgetanumber",
        ) => PpcImportDispatcherTarget::LMGetANumber,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMSetDABeeper" | "lmsetdabeeper",
        ) => PpcImportDispatcherTarget::LMSetDABeeper,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMGetDABeeper" | "lmgetdabeeper",
        ) => PpcImportDispatcherTarget::LMGetDABeeper,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMGetDAStrings" | "lmgetdastrings",
        ) => PpcImportDispatcherTarget::LMGetDAStrings,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMSetDlgFont" | "lmsetdlgfont",
        ) => PpcImportDispatcherTarget::LMSetDlgFont,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "LMGetDlgFont" | "lmgetdlgfont",
        ) => PpcImportDispatcherTarget::LMGetDlgFont,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMSetMenuFlash" | "lmsetmenuflash" | "SetMenuFlash" | "setmenuflash",
        ) => PpcImportDispatcherTarget::SetMenuFlash,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "ErrorSound",
        ) => PpcImportDispatcherTarget::ErrorSound,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ClearMenuBar" | "clearmenubar",
        ) => PpcImportDispatcherTarget::ClearMenuBar,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetMenuBar" | "setmenubar",
        ) => PpcImportDispatcherTarget::SetMenuBar,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetMenuHandle" | "getmenuhandle" | "GetMHandle" | "getmhandle",
        ) => PpcImportDispatcherTarget::GetMenuHandle,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DrawMenuBar" | "drawmenubar",
        ) => PpcImportDispatcherTarget::DrawMenuBar,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "FlashMenuBar" | "flashmenubar",
        ) => PpcImportDispatcherTarget::FlashMenuBar,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HMGetHelpMenuHandle" | "hmgethelpmenuhandle",
        ) => PpcImportDispatcherTarget::HMGetHelpMenuHandle,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HMGetBalloons" | "hmgetballoons",
        ) => PpcImportDispatcherTarget::HMGetBalloons,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HMIsBalloon" | "hmisballoon",
        ) => PpcImportDispatcherTarget::HMIsBalloon,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HMSetBalloons" | "hmsetballoons",
        ) => PpcImportDispatcherTarget::HMSetBalloons,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HiliteMenu" | "hilitemenu",
        ) => PpcImportDispatcherTarget::HiliteMenu,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InvalMenuBar" | "invalmenubar",
        ) => PpcImportDispatcherTarget::InvalMenuBar,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DrawGrowIcon" | "drawgrowicon",
        ) => PpcImportDispatcherTarget::DrawGrowIcon,
        ("InterfaceLib", "OpenCPicture") | ("InterfaceLib", "DebugStr") => {
            PpcImportDispatcherTarget::MenuNoop
        }
        ("InterfaceLib", "LocalToGlobal") => PpcImportDispatcherTarget::LocalToGlobal,
        ("InterfaceLib", "GlobalToLocal") => PpcImportDispatcherTarget::GlobalToLocal,
        ("InterfaceLib", "AddPt") => PpcImportDispatcherTarget::AddPt,
        ("InterfaceLib", "SubPt") => PpcImportDispatcherTarget::SubPt,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "MenuKey" | "menukey",
        ) => PpcImportDispatcherTarget::MenuKey,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "MenuEvent" | "menuevent",
        ) => PpcImportDispatcherTarget::MenuEvent,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "MenuChoice" | "menuchoice",
        ) => PpcImportDispatcherTarget::MenuChoice,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "MenuSelect" | "menuselect",
        ) => PpcImportDispatcherTarget::MenuSelect,
        ("InterfaceLib", "MoveTo") => PpcImportDispatcherTarget::MoveTo,
        ("InterfaceLib", "MovePortTo") => PpcImportDispatcherTarget::MovePortTo,
        ("InterfaceLib", "PortSize") => PpcImportDispatcherTarget::PortSize,
        ("InterfaceLib", "Move") => PpcImportDispatcherTarget::Move,
        ("InterfaceLib", "LineTo") => PpcImportDispatcherTarget::LineTo,
        ("InterfaceLib", "Line") => PpcImportDispatcherTarget::Line,
        ("InterfaceLib", "DrawChar") => PpcImportDispatcherTarget::DrawChar,
        ("InterfaceLib", "DrawText") => PpcImportDispatcherTarget::DrawText,
        ("InterfaceLib", "DrawString") => PpcImportDispatcherTarget::DrawString,
        ("InterfaceLib", "TextFont") => PpcImportDispatcherTarget::TextFont,
        ("InterfaceLib", "TextFace") => PpcImportDispatcherTarget::TextFace,
        ("InterfaceLib", "TextMode") => PpcImportDispatcherTarget::TextMode,
        ("InterfaceLib", "TextSize") => PpcImportDispatcherTarget::TextSize,
        ("InterfaceLib", "CharExtra") => PpcImportDispatcherTarget::CharExtra,
        ("InterfaceLib", "PaintRect") => PpcImportDispatcherTarget::PaintRect,
        ("InterfaceLib", "EraseRect") => PpcImportDispatcherTarget::EraseRect,
        ("InterfaceLib", "InvertRect") => PpcImportDispatcherTarget::InvertRect,
        ("InterfaceLib", "FrameRect") => PpcImportDispatcherTarget::FrameRect,
        ("InterfaceLib", "FillRect") => PpcImportDispatcherTarget::FillRect,
        ("InterfaceLib", "FrameOval") => PpcImportDispatcherTarget::FrameOval,
        ("InterfaceLib", "PaintOval") => PpcImportDispatcherTarget::PaintOval,
        ("InterfaceLib", "EraseOval") => PpcImportDispatcherTarget::EraseOval,
        ("InterfaceLib", "PaintArc") => PpcImportDispatcherTarget::PaintArc,
        ("InterfaceLib", "FrameRgn") => PpcImportDispatcherTarget::FrameRgn,
        ("InterfaceLib", "PaintRgn") => PpcImportDispatcherTarget::PaintRgn,
        ("InterfaceLib", "FillRgn") => PpcImportDispatcherTarget::FillRgn,
        ("InterfaceLib", "InvertRgn") => PpcImportDispatcherTarget::InvertRgn,
        ("InterfaceLib", "FillCRect") => PpcImportDispatcherTarget::FillCRect,
        ("InterfaceLib", "FrameRoundRect") => PpcImportDispatcherTarget::FrameRoundRect,
        ("InterfaceLib", "PaintRoundRect") => PpcImportDispatcherTarget::PaintRoundRect,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InvalRect" | "invalrect",
        ) => PpcImportDispatcherTarget::InvalRect,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InvalRgn" | "invalrgn",
        ) => PpcImportDispatcherTarget::InvalRgn,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ValidRect" | "validrect",
        ) => PpcImportDispatcherTarget::ValidRect,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ValidRgn" | "validrgn",
        ) => PpcImportDispatcherTarget::ValidRgn,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "BeginUpdate" | "beginupdate",
        ) => PpcImportDispatcherTarget::BeginUpdate,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "EndUpdate" | "endupdate",
        ) => PpcImportDispatcherTarget::EndUpdate,
        ("InterfaceLib", "ClipRect") => PpcImportDispatcherTarget::ClipRect,
        ("InterfaceLib", "GetClip") => PpcImportDispatcherTarget::GetClip,
        ("InterfaceLib", "SetClip") => PpcImportDispatcherTarget::SetClip,
        ("InterfaceLib", "GetPen") => PpcImportDispatcherTarget::GetPen,
        ("InterfaceLib", "HidePen") => PpcImportDispatcherTarget::HidePen,
        ("InterfaceLib", "ShowPen") => PpcImportDispatcherTarget::ShowPen,
        ("InterfaceLib", "PenSize") => PpcImportDispatcherTarget::PenSize,
        ("InterfaceLib", "PenMode") => PpcImportDispatcherTarget::PenMode,
        ("InterfaceLib", "PenNormal") => PpcImportDispatcherTarget::PenNormal,
        ("InterfaceLib", "PenPixPat") => PpcImportDispatcherTarget::PenPixPat,
        ("InterfaceLib", "GetPenState") => PpcImportDispatcherTarget::GetPenState,
        ("InterfaceLib", "SetPenState") => PpcImportDispatcherTarget::SetPenState,
        ("InterfaceLib", "CopyBits") => PpcImportDispatcherTarget::CopyBits,
        ("InterfaceLib", "UnpackBits") => PpcImportDispatcherTarget::UnpackBits,
        ("InterfaceLib", "BitMapToRegion") => PpcImportDispatcherTarget::BitMapToRegion,
        ("InterfaceLib", "NewRgn") => PpcImportDispatcherTarget::NewRgn,
        ("InterfaceLib", "DisposeRgn") => PpcImportDispatcherTarget::DisposeRgn,
        ("InterfaceLib", "CopyRgn") => PpcImportDispatcherTarget::CopyRgn,
        ("InterfaceLib", "OpenRgn") => PpcImportDispatcherTarget::OpenRgn,
        ("InterfaceLib", "CloseRgn") => PpcImportDispatcherTarget::CloseRgn,
        ("InterfaceLib", "SectRgn") => PpcImportDispatcherTarget::SectRgn,
        ("InterfaceLib", "UnionRgn") => PpcImportDispatcherTarget::UnionRgn,
        ("InterfaceLib", "DiffRgn") => PpcImportDispatcherTarget::DiffRgn,
        ("InterfaceLib", "XorRgn") => PpcImportDispatcherTarget::XorRgn,
        ("InterfaceLib", "SetEmptyRgn") => PpcImportDispatcherTarget::SetEmptyRgn,
        ("InterfaceLib", "SetRectRgn") => PpcImportDispatcherTarget::SetRectRgn,
        ("InterfaceLib", "RectRgn") => PpcImportDispatcherTarget::RectRgn,
        ("InterfaceLib", "OffsetRgn") => PpcImportDispatcherTarget::OffsetRgn,
        ("InterfaceLib", "EmptyRgn") => PpcImportDispatcherTarget::EmptyRgn,
        ("InterfaceLib", "PtInRgn") => PpcImportDispatcherTarget::PtInRgn,
        ("InterfaceLib", "RectInRgn") => PpcImportDispatcherTarget::RectInRgn,
        ("InterfaceLib", "OpenPoly") => PpcImportDispatcherTarget::OpenPoly,
        ("InterfaceLib", "ClosePoly") => PpcImportDispatcherTarget::ClosePoly,
        ("InterfaceLib", "KillPoly") => PpcImportDispatcherTarget::KillPoly,
        ("InterfaceLib", "PaintPoly") => PpcImportDispatcherTarget::PaintPoly,
        ("InterfaceLib", "FramePoly") => PpcImportDispatcherTarget::FramePoly,
        ("InterfaceLib", "FillPoly") => PpcImportDispatcherTarget::FillPoly,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "NewCWindow" | "newcwindow",
        ) => PpcImportDispatcherTarget::NewCWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetNewCWindow" | "getnewcwindow",
        ) => PpcImportDispatcherTarget::GetNewCWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWRefCon" | "getwrefcon" | "GetWindowRefCon" | "getwindowrefcon",
        ) => PpcImportDispatcherTarget::GetWRefCon,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWRefCon" | "setwrefcon" | "SetWindowRefCon" | "setwindowrefcon",
        ) => PpcImportDispatcherTarget::SetWRefCon,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowPic" | "getwindowpic",
        ) => PpcImportDispatcherTarget::GetWindowPic,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowPic" | "setwindowpic",
        ) => PpcImportDispatcherTarget::SetWindowPic,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetAuxWin" | "getauxwin",
        ) => PpcImportDispatcherTarget::GetAuxWin,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMGetWindowList" | "lmgetwindowlist",
        ) => PpcImportDispatcherTarget::LMGetWindowList,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMSetWindowList" | "lmsetwindowlist",
        ) => PpcImportDispatcherTarget::LMSetWindowList,
        ("InterfaceLib", "LMSetCurActivate") => PpcImportDispatcherTarget::LMSetCurActivate,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMGetAuxWinHead" | "lmgetauxwinhead",
        ) => PpcImportDispatcherTarget::LMGetAuxWinHead,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "LMSetAuxWinHead" | "lmsetauxwinhead",
        ) => PpcImportDispatcherTarget::LMSetAuxWinHead,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SizeWindow" | "sizewindow",
        ) => PpcImportDispatcherTarget::SizeWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "MoveWindow" | "movewindow",
        ) => PpcImportDispatcherTarget::MoveWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ShowWindow" | "showwindow",
        ) => PpcImportDispatcherTarget::ShowWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HideWindow" | "hidewindow",
        ) => PpcImportDispatcherTarget::HideWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ShowHide" | "showhide",
        ) => PpcImportDispatcherTarget::ShowHide,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CloseWindow" | "closewindow",
        ) => PpcImportDispatcherTarget::CloseWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SelectWindow" | "selectwindow",
        ) => PpcImportDispatcherTarget::SelectWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "FrontWindow" | "frontwindow",
        ) => PpcImportDispatcherTarget::FrontWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWinColor" | "setwincolor",
        ) => PpcImportDispatcherTarget::SetWinColor,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "PaintOne" | "paintone",
        ) => PpcImportDispatcherTarget::PaintOne,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "PaintBehind" | "paintbehind",
        ) => PpcImportDispatcherTarget::PaintBehind,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CalcVisBehind" | "calcvisbehind",
        ) => PpcImportDispatcherTarget::CalcVisBehind,
        ("InterfaceLib", "GetMouse") => PpcImportDispatcherTarget::GetMouse,
        ("InterfaceLib", "ActivatePalette") => PpcImportDispatcherTarget::ActivatePalette,
        ("InterfaceLib", "SetPalette") | ("InterfaceLib", "NSetPalette") => {
            PpcImportDispatcherTarget::NSetPalette
        }
        ("InterfaceLib", "GetPalette") => PpcImportDispatcherTarget::GetPalette,
        ("InterfaceLib", "GetPort") => PpcImportDispatcherTarget::GetPort,
        ("InterfaceLib", "GetPortBounds") => PpcImportDispatcherTarget::GetPortBounds,
        ("InterfaceLib", "GetWMgrPort") | ("InterfaceLib", "GetCWMgrPort") => {
            PpcImportDispatcherTarget::GetWMgrPort
        }
        ("InterfaceLib", "SetPort") => PpcImportDispatcherTarget::SetPort,
        ("InterfaceLib", "GetGDevice") => PpcImportDispatcherTarget::GetGDevice,
        ("InterfaceLib", "SetGDevice") => PpcImportDispatcherTarget::SetGDevice,
        ("InterfaceLib", "GetDeviceList") => PpcImportDispatcherTarget::GetDeviceList,
        ("InterfaceLib", "GetNextDevice") => PpcImportDispatcherTarget::GetNextDevice,
        ("InterfaceLib", "GetMainDevice") => PpcImportDispatcherTarget::GetMainDevice,
        ("InterfaceLib", "GetMaxDevice") => PpcImportDispatcherTarget::GetMaxDevice,
        ("InterfaceLib", "DeviceLoop") => PpcImportDispatcherTarget::DeviceLoop,
        ("InterfaceLib", "GetSysFont") => PpcImportDispatcherTarget::GetSysFont,
        ("InterfaceLib", "GetAppFont") => PpcImportDispatcherTarget::GetAppFont,
        ("InterfaceLib", "GetDefFontSize") => PpcImportDispatcherTarget::GetDefFontSize,
        ("InterfaceLib", "GetFontName") => PpcImportDispatcherTarget::GetFontName,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetMBarHeight" | "getmbarheight" | "LMGetMBarHeight" | "lmgetmbarheight",
        ) => PpcImportDispatcherTarget::GetMBarHeight,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetMBarHeight" | "setmbarheight" | "LMSetMBarHeight" | "lmsetmbarheight",
        ) => PpcImportDispatcherTarget::SetMBarHeight,
        ("InterfaceLib", "TestDeviceAttribute") => PpcImportDispatcherTarget::TestDeviceAttribute,
        ("InterfaceLib", "SetDeviceAttribute") => PpcImportDispatcherTarget::SetDeviceAttribute,
        ("InterfaceLib", "HasDepth") => PpcImportDispatcherTarget::HasDepth,
        ("InterfaceLib", "SetDepth") => PpcImportDispatcherTarget::SetDepth,
        ("InterfaceLib", "DMGetDisplayIDByGDevice") => {
            PpcImportDispatcherTarget::DMGetDisplayIDByGDevice
        }
        ("DisplayLib", "DMGetNameByAVID") => PpcImportDispatcherTarget::DMGetNameByAVID,
        ("InterfaceLib", "DMGetGDeviceByDisplayID") => {
            PpcImportDispatcherTarget::DMGetGDeviceByDisplayID
        }
        ("InterfaceLib" | "DisplayLib", "DMGetFirstScreenDevice") => {
            PpcImportDispatcherTarget::DMGetFirstScreenDevice
        }
        ("InterfaceLib" | "DisplayLib", "DMGetNextScreenDevice") => {
            PpcImportDispatcherTarget::DMGetNextScreenDevice
        }
        ("InterfaceLib" | "DisplayLib", "DMGetDisplayMode") => {
            PpcImportDispatcherTarget::DMGetDisplayMode
        }
        ("InterfaceLib" | "DisplayLib", "DMCheckDisplayMode") => {
            PpcImportDispatcherTarget::DMCheckDisplayMode
        }
        ("InterfaceLib" | "DisplayLib", "DMSetDisplayMode") => {
            PpcImportDispatcherTarget::DMSetDisplayMode
        }
        ("InterfaceLib" | "DisplayLib", "DMNewDisplayModeList") => {
            PpcImportDispatcherTarget::DMNewDisplayModeList
        }
        ("InterfaceLib" | "DisplayLib", "DMGetIndexedDisplayModeFromList") => {
            PpcImportDispatcherTarget::DMGetIndexedDisplayModeFromList
        }
        ("InterfaceLib" | "DisplayLib", "DMDisposeList") => {
            PpcImportDispatcherTarget::DMDisposeList
        }
        ("InterfaceLib" | "DisplayLib", "DMBeginConfigureDisplays") => {
            PpcImportDispatcherTarget::DMBeginConfigureDisplays
        }
        ("InterfaceLib" | "DisplayLib", "DMEndConfigureDisplays") => {
            PpcImportDispatcherTarget::DMEndConfigureDisplays
        }
        // Universal Interfaces 3.4.1 Displays.h: these configuration-session
        // controls return OSErr and do not select a display mode themselves.
        ("InterfaceLib" | "DisplayLib", "DMResumeConfigure")
        | ("InterfaceLib" | "DisplayLib", "DMSuspendConfigure")
        | ("InterfaceLib" | "DisplayLib", "DMUseScreenPrefs") => {
            PpcImportDispatcherTarget::ReturnNoErr
        }
        ("InterfaceLib", "NewGWorld") => PpcImportDispatcherTarget::NewGWorld,
        ("InterfaceLib", "UpdateGWorld") => PpcImportDispatcherTarget::UpdateGWorld,
        ("InterfaceLib", "DisposeGWorld") => PpcImportDispatcherTarget::DisposeGWorld,
        ("InterfaceLib", "GetCTable") => PpcImportDispatcherTarget::GetCTable,
        ("InterfaceLib", "GetCTSeed") => PpcImportDispatcherTarget::GetCTSeed,
        ("InterfaceLib", "MakeITable") => PpcImportDispatcherTarget::MakeITable,
        ("InterfaceLib", "QDError") => PpcImportDispatcherTarget::QDError,
        ("InterfaceLib", "QDDone") => PpcImportDispatcherTarget::QDDone,
        ("InterfaceLib", "CTabChanged") => PpcImportDispatcherTarget::CTabChanged,
        ("InterfaceLib", "ProtectEntry") => PpcImportDispatcherTarget::ProtectEntry,
        ("InterfaceLib", "ReserveEntry") => PpcImportDispatcherTarget::ReserveEntry,
        ("InterfaceLib", "RestoreEntries") => PpcImportDispatcherTarget::RestoreEntries,
        ("InterfaceLib", "SetEntries") => PpcImportDispatcherTarget::SetEntries,
        ("InterfaceLib", "RestoreDeviceClut") => PpcImportDispatcherTarget::RestoreDeviceClut,
        ("InterfaceLib", "DisposeCTable") => PpcImportDispatcherTarget::DisposeCTable,
        ("InterfaceLib", "NewPixMap") => PpcImportDispatcherTarget::NewPixMap,
        ("InterfaceLib", "DisposePixMap") | ("InterfaceLib", "DisposPixMap") => {
            PpcImportDispatcherTarget::DisposePixMap
        }
        ("InterfaceLib", "GetGWorld") => PpcImportDispatcherTarget::GetGWorld,
        ("InterfaceLib", "SetGWorld") => PpcImportDispatcherTarget::SetGWorld,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowPort" | "getwindowport",
        ) => PpcImportDispatcherTarget::GetWindowPort,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetPortWindowPort" | "setportwindowport",
        ) => PpcImportDispatcherTarget::SetPortWindowPort,
        ("InterfaceLib", "GetGWorldDevice") => PpcImportDispatcherTarget::GetGWorldDevice,
        ("InterfaceLib", "GetGWorldPixMap") => PpcImportDispatcherTarget::GetGWorldPixMap,
        ("InterfaceLib", "OpenPort") => PpcImportDispatcherTarget::OpenPort,
        ("InterfaceLib", "OpenCPort") => PpcImportDispatcherTarget::OpenCPort,
        ("InterfaceLib", "InitCPort") => PpcImportDispatcherTarget::InitCPort,
        ("InterfaceLib", "ClosePort") | ("InterfaceLib", "CloseCPort") => {
            PpcImportDispatcherTarget::CloseCPort
        }
        ("InterfaceLib", "SetPortBits") => PpcImportDispatcherTarget::SetPortBits { color: false },
        ("InterfaceLib", "SetPortPix") => PpcImportDispatcherTarget::SetPortBits { color: true },
        ("InterfaceLib", "GetPixBaseAddr") => PpcImportDispatcherTarget::GetPixBaseAddr,
        ("InterfaceLib", "GetPixRowBytes") => PpcImportDispatcherTarget::GetPixRowBytes,
        ("InterfaceLib", "LockPixels") => PpcImportDispatcherTarget::LockPixels,
        ("InterfaceLib", "UnlockPixels") => PpcImportDispatcherTarget::UnlockPixels,
        ("InterfaceLib", "GetPixelsState") => PpcImportDispatcherTarget::GetPixelsState,
        ("InterfaceLib", "SetPixelsState") => PpcImportDispatcherTarget::SetPixelsState,
        ("InterfaceLib", "AllowPurgePixels") => PpcImportDispatcherTarget::AllowPurgePixels,
        ("InterfaceLib", "NoPurgePixels") => PpcImportDispatcherTarget::NoPurgePixels,
        ("InterfaceLib", "SetRect") => PpcImportDispatcherTarget::SetRect,
        ("InterfaceLib", "SectRect") => PpcImportDispatcherTarget::SectRect,
        ("InterfaceLib", "UnionRect") => PpcImportDispatcherTarget::UnionRect,
        ("InterfaceLib", "EqualRect") => PpcImportDispatcherTarget::EqualRect,
        ("InterfaceLib", "EmptyRect") => PpcImportDispatcherTarget::EmptyRect,
        ("InterfaceLib", "SetPt") => PpcImportDispatcherTarget::SetPt,
        ("InterfaceLib", "EqualPt") => PpcImportDispatcherTarget::EqualPt,
        ("InterfaceLib", "PtInRect") => PpcImportDispatcherTarget::PtInRect,
        ("InterfaceLib", "SetOrigin") => PpcImportDispatcherTarget::SetOrigin,
        ("InterfaceLib", "OffsetRect") => PpcImportDispatcherTarget::OffsetRect,
        ("InterfaceLib", "MapRect") => PpcImportDispatcherTarget::MapRect,
        ("InterfaceLib", "InsetRect") => PpcImportDispatcherTarget::InsetRect,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "FindWindow" | "findwindow",
        ) => PpcImportDispatcherTarget::FindWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "PinRect" | "pinrect",
        ) => PpcImportDispatcherTarget::PinRect,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWVariant" | "getwvariant" | "GetWindowVariant" | "getwindowvariant",
        ) => PpcImportDispatcherTarget::GetWVariant,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ClipAbove" | "clipabove",
        ) => PpcImportDispatcherTarget::ClipAbove,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SaveOld" | "saveold",
        ) => PpcImportDispatcherTarget::SaveOld,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DrawNew" | "drawnew",
        ) => PpcImportDispatcherTarget::DrawNew,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DragGrayRgn" | "draggrayrgn" | "DragTheRgn" | "dragthergn",
        ) => PpcImportDispatcherTarget::DragGrayRgn,
        ("InterfaceLib", "GetGrayRgn") | ("InterfaceLib", "LMGetGrayRgn") => {
            PpcImportDispatcherTarget::GetGrayRgn
        }
        ("InterfaceLib", "LMSetGrayRgn") => PpcImportDispatcherTarget::LMSetGrayRgn,
        ("InterfaceLib", "GetDCtlEntry") => PpcImportDispatcherTarget::GetDCtlEntry,
        ("InterfaceLib", "GetADBInfo") => PpcImportDispatcherTarget::GetADBInfo,
        ("InterfaceLib", "AutoSleepControl") => PpcImportDispatcherTarget::AutoSleepControl,
        ("InterfaceLib", "IsAutoSlpControlDisabled") => {
            PpcImportDispatcherTarget::IsAutoSlpControlDisabled
        }
        ("InterfaceLib", "OpenDriver") => PpcImportDispatcherTarget::OpenDriver,
        ("InterfaceLib", "Control") => PpcImportDispatcherTarget::Control,
        ("InterfaceLib", "PBControl")
        | ("InterfaceLib", "PBControlSync")
        | ("InterfaceLib", "PBControlAsync") => PpcImportDispatcherTarget::PBControl,
        ("InterfaceLib", "PBStatus")
        | ("InterfaceLib", "PBStatusSync")
        | ("InterfaceLib", "PBStatusAsync") => PpcImportDispatcherTarget::PBStatus,
        ("InterfaceLib", "FindFolder") => PpcImportDispatcherTarget::FindFolder,
        ("InterfaceLib", "NewAlias") => PpcImportDispatcherTarget::NewAlias,
        ("InterfaceLib", "NewAliasMinimalFromFullPath") => {
            PpcImportDispatcherTarget::NewAliasMinimalFromFullPath
        }
        ("InterfaceLib", "UpdateAlias") => PpcImportDispatcherTarget::UpdateAlias,
        ("InterfaceLib", "ResolveAlias") => PpcImportDispatcherTarget::ResolveAlias,
        ("InterfaceLib", "ResolveAliasFile") => PpcImportDispatcherTarget::ResolveAliasFile,
        ("InterfaceLib", "ResolveAliasFileWithMountFlags") => {
            PpcImportDispatcherTarget::ResolveAliasFileWithMountFlags
        }
        ("InterfaceLib", "GetIconRefFromFile") => PpcImportDispatcherTarget::GetIconRefFromFile,
        ("InterfaceLib", "GetIconRef") => PpcImportDispatcherTarget::GetIconRef,
        ("InterfaceLib", "PlotIconRef") => PpcImportDispatcherTarget::PlotIconRef,
        ("InterfaceLib", "ReleaseIconRef") => PpcImportDispatcherTarget::ReleaseIconRef,
        ("InterfaceLib", "DirCreate") => PpcImportDispatcherTarget::DirCreate,
        ("InterfaceLib", "FSpDirCreate") => PpcImportDispatcherTarget::FSpDirCreate,
        ("InterfaceLib", "FSMakeFSSpec") => PpcImportDispatcherTarget::FSMakeFSSpec,
        ("InterfaceLib", "PBGetFInfo")
        | ("InterfaceLib", "PBGetFInfoSync")
        | ("InterfaceLib", "PBGetFInfoAsync") => PpcImportDispatcherTarget::PBGetFInfo,
        ("InterfaceLib", "PBHGetFInfo")
        | ("InterfaceLib", "PBHGetFInfoSync")
        | ("InterfaceLib", "PBHGetFInfoAsync") => PpcImportDispatcherTarget::PBHGetFInfo,
        ("InterfaceLib", "PBSetFInfo")
        | ("InterfaceLib", "PBSetFInfoSync")
        | ("InterfaceLib", "PBSetFInfoAsync") => PpcImportDispatcherTarget::PBSetFInfo,
        ("InterfaceLib", "PBHSetFInfo")
        | ("InterfaceLib", "PBHSetFInfoSync")
        | ("InterfaceLib", "PBHSetFInfoAsync") => PpcImportDispatcherTarget::PBHSetFInfo,
        ("InterfaceLib", "PBGetCatInfo")
        | ("InterfaceLib", "PBGetCatInfoSync")
        | ("InterfaceLib", "PBGetCatInfoAsync") => PpcImportDispatcherTarget::PBGetCatInfo,
        ("InterfaceLib", "PBSetCatInfo")
        | ("InterfaceLib", "PBSetCatInfoSync")
        | ("InterfaceLib", "PBSetCatInfoAsync") => PpcImportDispatcherTarget::PBSetCatInfo,
        ("InterfaceLib", "PBGetVInfo")
        | ("InterfaceLib", "PBGetVInfoSync")
        | ("InterfaceLib", "PBGetVInfoAsync")
        | ("InterfaceLib", "PBHGetVInfo")
        | ("InterfaceLib", "PBHGetVInfoSync")
        | ("InterfaceLib", "PBHGetVInfoAsync") => PpcImportDispatcherTarget::PBHGetVInfo,
        ("InterfaceLib", "GetVInfo") => PpcImportDispatcherTarget::GetVInfo,
        ("InterfaceLib", "GetVRefNum") => PpcImportDispatcherTarget::GetVRefNum,
        ("InterfaceLib", "PBDTGetPath") => PpcImportDispatcherTarget::PBDTGetPath,
        ("InterfaceLib", "PBDTGetCommentSync") => PpcImportDispatcherTarget::PBDTGetCommentSync,
        ("InterfaceLib", "PBGetFCBInfo")
        | ("InterfaceLib", "PBGetFCBInfoSync")
        | ("InterfaceLib", "PBGetFCBInfoAsync") => PpcImportDispatcherTarget::PBGetFCBInfo,
        ("InterfaceLib", "FSpGetFInfo") => PpcImportDispatcherTarget::FSpGetFInfo,
        ("InterfaceLib", "GetFInfo") | ("InterfaceLib", "getfinfo") => {
            PpcImportDispatcherTarget::GetFInfo
        }
        ("InterfaceLib", "HGetFInfo") => PpcImportDispatcherTarget::HGetFInfo,
        ("InterfaceLib", "FSpSetFInfo") => PpcImportDispatcherTarget::FSpSetFInfo,
        ("InterfaceLib", "HSetFInfo") => PpcImportDispatcherTarget::HSetFInfo,
        ("InterfaceLib", "StandardGetFile") => PpcImportDispatcherTarget::StandardGetFile,
        ("InterfaceLib", "GetScrap") => PpcImportDispatcherTarget::GetScrap,
        ("InterfaceLib", "PutScrap") => PpcImportDispatcherTarget::PutScrap,
        ("InterfaceLib", "ZeroScrap") => PpcImportDispatcherTarget::ZeroScrap,
        ("InterfaceLib", "LoadScrap") => PpcImportDispatcherTarget::LoadScrap,
        ("InterfaceLib", "UnloadScrap") => PpcImportDispatcherTarget::UnloadScrap,
        ("InterfaceLib", "FSpOpenDF") => PpcImportDispatcherTarget::FSpOpenDF,
        ("InterfaceLib", "FSpOpenRF") => PpcImportDispatcherTarget::FSpOpenRF,
        ("InterfaceLib", "HOpen") | ("InterfaceLib", "HOpenDF") => PpcImportDispatcherTarget::HOpen,
        ("InterfaceLib", "FSOpen") => PpcImportDispatcherTarget::FSOpen,
        ("InterfaceLib", "PBOpen")
        | ("InterfaceLib", "PBOpenSync")
        | ("InterfaceLib", "PBOpenAsync") => PpcImportDispatcherTarget::PBOpen,
        ("InterfaceLib", "PBHOpenDF")
        | ("InterfaceLib", "PBHOpenDFSync")
        | ("InterfaceLib", "PBHOpenDFAsync")
        | ("InterfaceLib", "PBHOpen")
        | ("InterfaceLib", "PBHOpenSync")
        | ("InterfaceLib", "PBHOpenAsync") => PpcImportDispatcherTarget::PBHOpenDF,
        ("InterfaceLib", "FSpCreateResFile") => PpcImportDispatcherTarget::FSpCreateResFile,
        ("InterfaceLib", "HCreateResFile") => PpcImportDispatcherTarget::HCreateResFile,
        ("InterfaceLib", "FSpOpenResFile") => PpcImportDispatcherTarget::FSpOpenResFile,
        ("InterfaceLib", "FSClose") => PpcImportDispatcherTarget::FSClose,
        ("InterfaceLib", "PBClose")
        | ("InterfaceLib", "PBCloseSync")
        | ("InterfaceLib", "PBCloseAsync") => PpcImportDispatcherTarget::PBClose,
        ("InterfaceLib", "PBFlushFile")
        | ("InterfaceLib", "PBFlushFileSync")
        | ("InterfaceLib", "PBFlushFileAsync") => PpcImportDispatcherTarget::PBFlushFile,
        ("InterfaceLib", "FSRead") => PpcImportDispatcherTarget::FSRead,
        ("InterfaceLib", "PBRead")
        | ("InterfaceLib", "PBReadSync")
        | ("InterfaceLib", "PBReadAsync") => PpcImportDispatcherTarget::PBRead,
        ("InterfaceLib", "FSWrite") => PpcImportDispatcherTarget::FSWrite,
        ("InterfaceLib", "PBWrite")
        | ("InterfaceLib", "PBWriteSync")
        | ("InterfaceLib", "PBWriteAsync") => PpcImportDispatcherTarget::PBWrite,
        ("InterfaceLib", "GetEOF") => PpcImportDispatcherTarget::GetEOF,
        ("InterfaceLib", "PBGetEOF")
        | ("InterfaceLib", "PBGetEOFSync")
        | ("InterfaceLib", "PBGetEOFAsync") => PpcImportDispatcherTarget::PBGetEOF,
        ("InterfaceLib", "SetEOF") => PpcImportDispatcherTarget::SetEOF,
        ("InterfaceLib", "AllocContig") | ("InterfaceLib", "Allocate") => {
            PpcImportDispatcherTarget::AllocContig
        }
        ("InterfaceLib", "PBSetEOF")
        | ("InterfaceLib", "PBSetEOFSync")
        | ("InterfaceLib", "PBSetEOFAsync") => PpcImportDispatcherTarget::PBSetEOF,
        ("InterfaceLib", "GetFPos") => PpcImportDispatcherTarget::GetFPos,
        ("InterfaceLib", "SetFPos") => PpcImportDispatcherTarget::SetFPos,
        ("InterfaceLib", "PBSetFPos")
        | ("InterfaceLib", "PBSetFPosSync")
        | ("InterfaceLib", "PBSetFPosAsync") => PpcImportDispatcherTarget::PBSetFPos,
        ("InterfaceLib", "FSpCreate") => PpcImportDispatcherTarget::FSpCreate,
        ("InterfaceLib", "PBHCreate" | "PBHCreateSync" | "PBHCreateAsync") => {
            PpcImportDispatcherTarget::PBCreate(PpcParameterBlockCreateOperation::Hierarchical)
        }
        ("InterfaceLib", "PBCreate")
        | ("InterfaceLib", "PBCreateSync")
        | ("InterfaceLib", "PBCreateAsync") => {
            PpcImportDispatcherTarget::PBCreate(PpcParameterBlockCreateOperation::Legacy)
        }
        ("InterfaceLib", "FSpDelete") => PpcImportDispatcherTarget::FSpDelete,
        ("InterfaceLib", "HDelete") => {
            PpcImportDispatcherTarget::DeleteByName(PpcDeleteByNameOperation::HierarchicalHighLevel)
        }
        ("InterfaceLib", "FSDelete") => {
            PpcImportDispatcherTarget::DeleteByName(PpcDeleteByNameOperation::LegacyHighLevel)
        }
        ("InterfaceLib", "PBDelete")
        | ("InterfaceLib", "PBDeleteSync")
        | ("InterfaceLib", "PBDeleteAsync") => {
            PpcImportDispatcherTarget::DeleteByName(PpcDeleteByNameOperation::LegacyParameterBlock)
        }
        ("InterfaceLib", "PBHDelete" | "PBHDeleteSync" | "PBHDeleteAsync") => {
            PpcImportDispatcherTarget::DeleteByName(
                PpcDeleteByNameOperation::HierarchicalParameterBlock,
            )
        }
        ("InterfaceLib", "HCreate") => PpcImportDispatcherTarget::HCreate,
        ("InterfaceLib", "HRename") => PpcImportDispatcherTarget::HRename,
        ("InterfaceLib", "Create") => PpcImportDispatcherTarget::Create,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetNewDialog",
        ) => PpcImportDispatcherTarget::GetNewDialog,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "NewDialog" | "NewColorDialog" | "NewCDialog",
        ) => PpcImportDispatcherTarget::NewDialog,
        ("InterfaceLib" | "AppearanceLib", "RegisterAppearanceClient") => {
            PpcImportDispatcherTarget::RegisterAppearanceClient
        }
        ("InterfaceLib" | "AppearanceLib", "UnregisterAppearanceClient") => {
            PpcImportDispatcherTarget::UnregisterAppearanceClient
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ActivateControl" | "activatecontrol",
        ) => PpcImportDispatcherTarget::ActivateControl,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DeactivateControl" | "deactivatecontrol",
        ) => PpcImportDispatcherTarget::DeactivateControl,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsControlActive" | "iscontrolactive",
        ) => PpcImportDispatcherTarget::IsControlActive,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlFontStyle" | "setcontrolfontstyle",
        ) => PpcImportDispatcherTarget::SetControlFontStyle,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsControlVisible" | "iscontrolvisible",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlVisible),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsControlEnabled" | "iscontrolenabled",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlEnabled),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "EnableControl" | "enablecontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::EnableControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DisableControl" | "disablecontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisableControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsControlHilited" | "iscontrolhilited",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlHilited),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlHilite" | "getcontrolhilite",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlHilite),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsValidControlHandle"
            | "isvalidcontrolhandle"
            | "IsValidControlRef"
            | "isvalidcontrolref",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsValidControlHandle),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlBounds" | "getcontrolbounds",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlBounds),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlBounds" | "setcontrolbounds",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlBounds),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IdleControls" | "idlecontrols",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IdleControls),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DragControl" | "dragcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DragControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlData" | "getcontroldata",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlData),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlData" | "setcontroldata",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlData),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlDataSize" | "getcontroldatasize",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlDataSize),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlFeatures" | "getcontrolfeatures",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlFeatures),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetBestControlRect" | "getbestcontrolrect",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetBestControlRect),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlVisibility" | "setcontrolvisibility",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlVisibility),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlColorProc" | "setcontrolcolorproc",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlColorProc),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlColorProc" | "getcontrolcolorproc",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlColorProc),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DrawControlInCurrentPort" | "drawcontrolincurrentport",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawControlInCurrentPort),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetUpControlBackground" | "setupcontrolbackground",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetUpControlBackground),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlOwner" | "getcontrolowner",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlOwner),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlRegion" | "getcontrolregion",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlRegion),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlID" | "setcontrolid",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlID),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlID" | "getcontrolid",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlID),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlCommandID" | "setcontrolcommandid",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlCommandID),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlCommandID" | "getcontrolcommandid",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlCommandID),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetKeyboardFocus" | "setkeyboardfocus",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetKeyboardFocus),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetKeyboardFocus" | "getkeyboardfocus",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetKeyboardFocus),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "AdvanceKeyboardFocus" | "advancekeyboardfocus",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::AdvanceKeyboardFocus),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ReverseKeyboardFocus" | "reversekeyboardfocus",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ReverseKeyboardFocus),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ClearKeyboardFocus" | "clearkeyboardfocus",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ClearKeyboardFocus),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlByID" | "getcontrolbyid",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlByID),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "FindControlUnderMouse" | "findcontrolundermouse",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::FindControlUnderMouse),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HandleControlClick" | "handlecontrolclick",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::HandleControlClick),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HandleControlKey" | "handlecontrolkey",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::HandleControlKey),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlClickActivation" | "getcontrolclickactivation",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlClickActivation),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlKind" | "getcontrolkind",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlKind),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlFocusPart" | "setcontrolfocuspart",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlFocusPart),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlFocusPart" | "getcontrolfocuspart",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlFocusPart),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SendControlMessage" | "sendcontrolmessage",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SendControlMessage),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ScrollControlValues" | "scrollcontrolvalues",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ScrollControlValues),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsControlDragTrackingEnabled" | "iscontroldragtrackingenabled",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::IsControlDragTrackingEnabled),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlDragTrackingEnabled" | "setcontroldragtrackingenabled",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlDragTrackingEnabled),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CollapseWindow" | "collapsewindow",
        ) => PpcImportDispatcherTarget::CollapseWindow,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsWindowCollapsed" | "iswindowcollapsed",
        ) => PpcImportDispatcherTarget::IsWindowCollapsed,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "NewFeaturesDialog",
        ) => {
            PpcImportDispatcherTarget::NewFeaturesDialog
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogItem" | "GetDItem" | "getdialogitem" | "getditem",
        ) => PpcImportDispatcherTarget::GetDialogItem,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogItemAsControl" | "getdialogitemascontrol",
        ) => PpcImportDispatcherTarget::GetDialogItemAsControl,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogItem" | "SetDItem" | "setdialogitem" | "setditem",
        ) => PpcImportDispatcherTarget::SetDialogItem,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogItemText" | "getdialogitemtext" | "GetIText" | "getitext",
        ) => PpcImportDispatcherTarget::GetDialogItemText,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogItemText" | "setdialogitemtext" | "SetIText" | "setitext",
        ) => PpcImportDispatcherTarget::SetDialogItemText,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogDefaultItem" | "setdialogdefaultitem",
        ) => PpcImportDispatcherTarget::SetDialogDefaultItem,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogDefaultItem" | "getdialogdefaultitem",
        ) => PpcImportDispatcherTarget::GetDialogDefaultItem,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogCancelItem" | "setdialogcancelitem",
        ) => PpcImportDispatcherTarget::SetDialogCancelItem,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogCancelItem" | "getdialogcancelitem",
        ) => PpcImportDispatcherTarget::GetDialogCancelItem,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogTracksCursor" | "setdialogtrackscursor",
        ) => PpcImportDispatcherTarget::SetDialogTracksCursor,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "MoveDialogItem" | "movedialogitem",
        ) => PpcImportDispatcherTarget::MoveDialogItem,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SizeDialogItem" | "sizedialogitem",
        ) => PpcImportDispatcherTarget::SizeDialogItem,
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "AppendDialogItemList") => {
            PpcImportDispatcherTarget::AppendDialogItemList
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "AutoSizeDialog") => {
            PpcImportDispatcherTarget::AutoSizeDialog
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CouldDialog",
        ) => PpcImportDispatcherTarget::CouldDialog,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "FreeDialog",
        ) => PpcImportDispatcherTarget::FreeDialog,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CouldAlert",
        ) => PpcImportDispatcherTarget::CouldAlert,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "FreeAlert",
        ) => PpcImportDispatcherTarget::FreeAlert,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "StdFilterProc",
        ) => PpcImportDispatcherTarget::StdFilterProc,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetStdFilterProc",
        ) => PpcImportDispatcherTarget::GetStdFilterProc,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetAlertStage",
        ) => {
            PpcImportDispatcherTarget::GetAlertStage
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogFont" | "SetDAFont",
        ) => {
            PpcImportDispatcherTarget::SetDialogFont
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "GetDialogPort") => {
            PpcImportDispatcherTarget::GetDialogPort
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "GetDialogWindow") => {
            PpcImportDispatcherTarget::GetDialogWindow
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "GetDialogFromWindow") => {
            PpcImportDispatcherTarget::GetDialogFromWindow
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "SetPortDialogPort") => {
            PpcImportDispatcherTarget::SetPortDialogPort
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "DrawDialog",
        ) => PpcImportDispatcherTarget::DrawDialog,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DrawControls" | "drawcontrols",
        ) => PpcImportDispatcherTarget::DrawControls,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "UpdateControls" | "updatecontrols",
        ) => PpcImportDispatcherTarget::UpdateControls,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "ModalDialog",
        ) => PpcImportDispatcherTarget::ModalDialog,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlTitle" | "setcontroltitle" | "SetCTitle" | "setctitle",
        ) => PpcImportDispatcherTarget::SetControlTitle,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlValue" | "setcontrolvalue" | "SetCtlValue" | "setctlvalue",
        ) => PpcImportDispatcherTarget::SetControlValue,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HiliteControl" | "hilitecontrol",
        ) => PpcImportDispatcherTarget::HiliteControl,
        ("InterfaceLib", "InitGraf") => PpcImportDispatcherTarget::InitGraf,
        ("InterfaceLib", "InitFonts") => PpcImportDispatcherTarget::InitFonts,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InitWindows" | "initwindows",
        ) => PpcImportDispatcherTarget::InitWindows,
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InitMenus" | "initmenus",
        ) => PpcImportDispatcherTarget::InitMenus,
        ("InterfaceLib", "TEInit") => PpcImportDispatcherTarget::TEInit,
        ("InterfaceLib", "TENew") => PpcImportDispatcherTarget::TENew,
        ("InterfaceLib", "TEStyleNew") => PpcImportDispatcherTarget::TEStyleNew,
        ("InterfaceLib", "TESetStyle") => PpcImportDispatcherTarget::TESetStyle,
        ("InterfaceLib", "TEUseStyleScrap") => PpcImportDispatcherTarget::TEUseStyleScrap,
        ("InterfaceLib", "TEContinuousStyle") => PpcImportDispatcherTarget::TEContinuousStyle,
        ("InterfaceLib", "TEGetText") => PpcImportDispatcherTarget::TEGetText,
        ("InterfaceLib", "TEDispose") | ("InterfaceLib", "TEDispos") => {
            PpcImportDispatcherTarget::TEDispose
        }
        ("InterfaceLib", "TEActivate") | ("InterfaceLib", "TEActivat") => {
            PpcImportDispatcherTarget::TEActivate { active: true }
        }
        ("InterfaceLib", "TEDeactivate") | ("InterfaceLib", "TEDeactivat") => {
            PpcImportDispatcherTarget::TEActivate { active: false }
        }
        ("InterfaceLib", "TESetSelect") => PpcImportDispatcherTarget::TESetSelect,
        ("InterfaceLib", "TESetText") => PpcImportDispatcherTarget::TESetText,
        ("InterfaceLib", "TECalText") => PpcImportDispatcherTarget::TECalText,
        ("InterfaceLib", "TEInsert") => PpcImportDispatcherTarget::TEInsert { styled: false },
        ("InterfaceLib", "TEStyleInsert") => PpcImportDispatcherTarget::TEInsert { styled: true },
        ("InterfaceLib", "TEDelete") => PpcImportDispatcherTarget::TEDelete { dialog: false },
        ("InterfaceLib", "TEKey") => PpcImportDispatcherTarget::TEKey,
        ("InterfaceLib", "TEClick") => PpcImportDispatcherTarget::TEClick,
        ("InterfaceLib", "TEIdle") => PpcImportDispatcherTarget::TEIdle,
        ("InterfaceLib", "TEUpdate") => PpcImportDispatcherTarget::TEUpdate,
        ("InterfaceLib", "TETextBox") => PpcImportDispatcherTarget::TETextBox,
        ("InterfaceLib", "TESetAlignment") | ("InterfaceLib", "TESetJust") => {
            PpcImportDispatcherTarget::TESetAlignment
        }
        ("InterfaceLib", "TEGetHeight") => PpcImportDispatcherTarget::TEGetHeight,
        ("InterfaceLib", "TEGetPoint") => PpcImportDispatcherTarget::TEGetPoint,
        ("InterfaceLib", "TEScroll") => PpcImportDispatcherTarget::TEScroll { pinned: false },
        ("InterfaceLib", "TEPinScroll") => PpcImportDispatcherTarget::TEScroll { pinned: true },
        ("InterfaceLib", "TEAutoView") => PpcImportDispatcherTarget::TEAutoView,
        ("InterfaceLib", "TECopy") => PpcImportDispatcherTarget::TECopy {
            cut: false,
            dialog: false,
        },
        ("InterfaceLib", "TECut") => PpcImportDispatcherTarget::TECopy {
            cut: true,
            dialog: false,
        },
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "DialogCopy" | "DlgCopy") => {
            PpcImportDispatcherTarget::TECopy {
                cut: false,
                dialog: true,
            }
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "DialogCut" | "DlgCut") => {
            PpcImportDispatcherTarget::TECopy {
                cut: true,
                dialog: true,
            }
        }
        ("InterfaceLib", "TEPaste" | "TEStylePaste") => {
            PpcImportDispatcherTarget::TEPaste { dialog: false }
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "DialogPaste" | "DlgPaste") => {
            PpcImportDispatcherTarget::TEPaste { dialog: true }
        }
        ("InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib", "DialogDelete" | "DlgDelete") => {
            PpcImportDispatcherTarget::TEDelete { dialog: true }
        }
        ("InterfaceLib", "TEToScrap") => PpcImportDispatcherTarget::TETransferScrap {
            from_desktop: false,
        },
        ("InterfaceLib", "TEFromScrap") => {
            PpcImportDispatcherTarget::TETransferScrap { from_desktop: true }
        }
        ("InterfaceLib", "TEScrapHandle") => PpcImportDispatcherTarget::TEScrapHandle,
        ("InterfaceLib", "LMGetTEScrpLength" | "TEGetScrapLength") => {
            PpcImportDispatcherTarget::TEScrapLength { set: false }
        }
        ("InterfaceLib", "LMSetTEScrpLength") => {
            PpcImportDispatcherTarget::TEScrapLength { set: true }
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SelectDialogItemText" | "SelIText" | "selectdialogitemtext" | "selitext",
        ) => PpcImportDispatcherTarget::SelectDialogItemText,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "InitDialogs",
        ) => PpcImportDispatcherTarget::InitDialogs,
        ("InterfaceLib", "SystemTask") => PpcImportDispatcherTarget::SystemTask,
        ("InterfaceLib", "SystemClick") => PpcImportDispatcherTarget::SystemClick,
        ("InterfaceLib", "OpenDeskAcc") => PpcImportDispatcherTarget::OpenDeskAcc,
        ("InterfaceLib", "AEProcessAppleEvent") => PpcImportDispatcherTarget::AEProcessAppleEvent,
        ("InterfaceLib", "LNew") => PpcImportDispatcherTarget::LNew,
        ("InterfaceLib", "LDispose") => PpcImportDispatcherTarget::LDispose,
        ("InterfaceLib", "LAddRow") => PpcImportDispatcherTarget::LAddRow,
        ("InterfaceLib", "LDelRow") => PpcImportDispatcherTarget::LDelRow,
        ("InterfaceLib", "LGetSelect") => PpcImportDispatcherTarget::LGetSelect,
        ("InterfaceLib", "LSetSelect") => PpcImportDispatcherTarget::LSetSelect,
        ("InterfaceLib", "LSetCell") => PpcImportDispatcherTarget::LSetCell,
        ("InterfaceLib", "LGetCell") => PpcImportDispatcherTarget::LGetCell,
        ("InterfaceLib", "LClick") => PpcImportDispatcherTarget::LClick,
        ("InterfaceLib", "LActivate") => PpcImportDispatcherTarget::LActivate,
        ("InterfaceLib", "LSetDrawingMode") => PpcImportDispatcherTarget::LSetDrawingMode,
        ("InterfaceLib", "LScroll") => PpcImportDispatcherTarget::LScroll,
        ("InterfaceLib", "LSize") => PpcImportDispatcherTarget::LSize,
        ("InterfaceLib", "LUpdate") => PpcImportDispatcherTarget::LUpdate,
        ("InterfaceLib", "LAutoScroll") => PpcImportDispatcherTarget::LAutoScroll,
        ("InterfaceLib", "LSearch") => PpcImportDispatcherTarget::LSearch,
        ("InterfaceLib", "FlushEvents") => PpcImportDispatcherTarget::FlushEvents,
        ("InterfaceLib", "GetMainEventQueue") => PpcImportDispatcherTarget::GetMainEventQueue,
        ("InterfaceLib" | "CarbonLib", "GetMainEventLoop" | "GetCurrentEventLoop") => {
            PpcImportDispatcherTarget::GetMainEventLoop
        }
        ("InterfaceLib" | "CarbonLib", "InstallEventLoopTimer") => {
            PpcImportDispatcherTarget::InstallEventLoopTimer
        }
        ("InterfaceLib" | "CarbonLib", "RemoveEventLoopTimer") => {
            PpcImportDispatcherTarget::RemoveEventLoopTimer
        }
        ("InterfaceLib" | "CarbonLib", "GetApplicationEventTarget") => {
            PpcImportDispatcherTarget::GetApplicationEventTarget
        }
        ("InterfaceLib" | "CarbonLib", "GetEventDispatcherTarget") => {
            PpcImportDispatcherTarget::GetEventDispatcherTarget
        }
        ("InterfaceLib" | "CarbonLib", "InstallEventHandler") => {
            PpcImportDispatcherTarget::InstallEventHandler
        }
        ("InterfaceLib" | "CarbonLib", "RemoveEventHandler") => {
            PpcImportDispatcherTarget::RemoveEventHandler
        }
        ("InterfaceLib" | "CarbonLib", "CreateEvent") => PpcImportDispatcherTarget::CreateEvent,
        ("InterfaceLib" | "CarbonLib", "ReleaseEvent") => PpcImportDispatcherTarget::ReleaseEvent,
        ("InterfaceLib" | "CarbonLib", "RetainEvent") => PpcImportDispatcherTarget::RetainEvent,
        ("InterfaceLib" | "CarbonLib", "GetEventClass") => PpcImportDispatcherTarget::GetEventClass,
        ("InterfaceLib" | "CarbonLib", "GetEventKind") => PpcImportDispatcherTarget::GetEventKind,
        ("InterfaceLib" | "CarbonLib", "GetEventTime") => PpcImportDispatcherTarget::GetEventTime,
        ("InterfaceLib" | "CarbonLib", "SetEventParameter") => PpcImportDispatcherTarget::SetEventParameter,
        ("InterfaceLib" | "CarbonLib", "GetEventParameter") => PpcImportDispatcherTarget::GetEventParameter,
        ("InterfaceLib" | "CarbonLib", "PostEventToQueue") => PpcImportDispatcherTarget::PostEventToQueue,
        ("InterfaceLib" | "CarbonLib", "ReceiveNextEvent") => PpcImportDispatcherTarget::ReceiveNextEvent,
        ("InterfaceLib" | "CarbonLib", "SendEventToEventTarget") => PpcImportDispatcherTarget::SendEventToEventTarget,
        ("InterfaceLib" | "CarbonLib", "CallNextEventHandler") => PpcImportDispatcherTarget::CallNextEventHandler,
        ("InterfaceLib" | "CarbonLib", "RunApplicationEventLoop") => PpcImportDispatcherTarget::RunApplicationEventLoop,
        ("InterfaceLib" | "CarbonLib", "QuitApplicationEventLoop") => PpcImportDispatcherTarget::QuitApplicationEventLoop,
        ("InterfaceLib" | "CarbonLib", "InstallStandardEventHandler") => PpcImportDispatcherTarget::InstallStandardEventHandler,
        ("InterfaceLib" | "CarbonLib", "GetCurrentEventTime") => PpcImportDispatcherTarget::GetCurrentEventTime,
        ("InterfaceLib", "FlushEventQueue") => PpcImportDispatcherTarget::FlushEventQueue,
        ("InterfaceLib", "SetEventMask") => PpcImportDispatcherTarget::SetEventMask,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CloseDialog",
        ) => PpcImportDispatcherTarget::CloseDialog,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "DisposeDialog" | "DisposDialog",
        ) => {
            PpcImportDispatcherTarget::DisposeDialog
        }
        ("InterfaceLib" | "CarbonLib", "GetNextEvent") => {
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent)
        }
        // WaitNextEvent(eventMask, theEvent, sleep, mouseRgn) returns the
        // next matching event and yields time when no event is pending.
        // Macintosh Toolbox Essentials (1992), pp. 2-22–2-25.
        ("InterfaceLib" | "CarbonLib", "WaitNextEvent") => {
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)
        }
        ("InterfaceLib", "GetOSEvent") => PpcImportDispatcherTarget::GetOSEvent,
        ("InterfaceLib" | "CarbonLib", "EventAvail") => PpcImportDispatcherTarget::EventAvail,
        ("InterfaceLib", "OSEventAvail") => PpcImportDispatcherTarget::OSEventAvail,
        ("InterfaceLib", "PostEvent") => PpcImportDispatcherTarget::PostEvent,
        ("InterfaceLib", "Button") => PpcImportDispatcherTarget::Button,
        ("InterfaceLib", "StillDown") => PpcImportDispatcherTarget::StillDown,
        ("InterfaceLib", "WaitMouseUp") => PpcImportDispatcherTarget::WaitMouseUp,
        ("InterfaceLib", "GetKeys") => PpcImportDispatcherTarget::GetKeys,
        ("InterfaceLib", "GetDateTime") => PpcImportDispatcherTarget::GetDateTime,
        ("InterfaceLib", "ReadDateTime") => PpcImportDispatcherTarget::ReadDateTime,
        ("InterfaceLib", "ReadLocation") => PpcImportDispatcherTarget::ReadLocation,
        ("InterfaceLib", "GetTime") => PpcImportDispatcherTarget::GetTime,
        ("InterfaceLib", "Delay") => PpcImportDispatcherTarget::Delay,
        ("InterfaceLib", "GetDblTime") => PpcImportDispatcherTarget::GetDblTime,
        ("InterfaceLib", "LMGetTime") => PpcImportDispatcherTarget::LMGetTime,
        ("InterfaceLib", "LMGetUTableBase") => PpcImportDispatcherTarget::LMGetUTableBase,
        ("InterfaceLib", "LMGetCurDirStore") => PpcImportDispatcherTarget::LMGetCurDirStore,
        ("InterfaceLib", "LMSetCurDirStore") => PpcImportDispatcherTarget::LMSetCurDirStore,
        ("InterfaceLib", "LMGetSFSaveDisk") => PpcImportDispatcherTarget::LMGetSFSaveDisk,
        ("InterfaceLib", "LMSetSFSaveDisk") => PpcImportDispatcherTarget::LMSetSFSaveDisk,
        ("InterfaceLib", "LMGetRndSeed") => PpcImportDispatcherTarget::LMGetRndSeed,
        ("InterfaceLib", "LMSetRndSeed") => PpcImportDispatcherTarget::LMSetRndSeed,
        ("InterfaceLib", "SetCurrentA5") => PpcImportDispatcherTarget::SetCurrentA5,
        ("InterfaceLib", "SetA5") => PpcImportDispatcherTarget::SetA5,
        ("InterfaceLib", "SecondsToDate") | ("InterfaceLib", "Secs2Date") => {
            PpcImportDispatcherTarget::SecondsToDate
        }
        ("InterfaceLib", "Microseconds") => PpcImportDispatcherTarget::Microseconds,
        // AbsoluteTime is hardware-relative. Use the virtual microsecond
        // clock as its unit so this struct-returning call stays deterministic.
        ("InterfaceLib" | "DriverServicesLib", "UpTime") => {
            PpcImportDispatcherTarget::Microseconds
        }
        ("InterfaceLib" | "DriverServicesLib", "AbsoluteToNanoseconds") => {
            PpcImportDispatcherTarget::AbsoluteToNanoseconds
        }
        ("InterfaceLib", "LMGetTicks") => PpcImportDispatcherTarget::TickCount,
        ("InterfaceLib", "SysEnvirons") => PpcImportDispatcherTarget::SysEnvirons,
        ("InterfaceLib", "TextWidth") => PpcImportDispatcherTarget::TextWidth,
        ("InterfaceLib", "StringWidth") => PpcImportDispatcherTarget::StringWidth,
        ("InterfaceLib", "TruncString") => PpcImportDispatcherTarget::TruncString,
        ("InterfaceLib", "CharWidth") => PpcImportDispatcherTarget::CharWidth,
        ("InterfaceLib", "MeasureText") => PpcImportDispatcherTarget::MeasureText,
        ("InterfaceLib", "RealFont") => PpcImportDispatcherTarget::RealFont,
        ("InterfaceLib", "GetFontInfo") => PpcImportDispatcherTarget::GetFontInfo,
        ("InterfaceLib", "FontMetrics") => PpcImportDispatcherTarget::FontMetrics,
        ("InterfaceLib", "GetFNum") => PpcImportDispatcherTarget::GetFNum,
        ("InterfaceLib", "GetIntlResource") => PpcImportDispatcherTarget::GetIntlResource,
        ("InterfaceLib", "AESetInteractionAllowed") => {
            PpcImportDispatcherTarget::AESetInteractionAllowed
        }
        ("InterfaceLib", "AEGetInteractionAllowed") => {
            PpcImportDispatcherTarget::AEGetInteractionAllowed
        }
        ("InterfaceLib", "AEManagerInfo") => PpcImportDispatcherTarget::AEManagerInfo,
        ("InterfaceLib", "SVersion") => PpcImportDispatcherTarget::SVersion,
        ("InterfaceLib", "EqualString") => PpcImportDispatcherTarget::EqualString,
        ("InterfaceLib", "IUEqualPString") => PpcImportDispatcherTarget::IUEqualPString,
        ("InterfaceLib", "GetIntlResourceTable") => {
            PpcImportDispatcherTarget::GetIntlResourceTable
        }
        ("InterfaceLib", "NumToString") => PpcImportDispatcherTarget::NumToString,
        ("InterfaceLib", "StringToNum") => PpcImportDispatcherTarget::StringToNum,
        ("InterfaceLib", "Random") => PpcImportDispatcherTarget::Random,
        ("InterfaceLib", "BitAnd") => PpcImportDispatcherTarget::BitAnd,
        ("InterfaceLib", "BitOr") => PpcImportDispatcherTarget::BitOr,
        ("InterfaceLib", "BitTst") => PpcImportDispatcherTarget::BitTst,
        ("InterfaceLib", "p2cstr") | ("InterfaceLib", "P2CStr") => {
            PpcImportDispatcherTarget::P2CStr
        }
        ("InterfaceLib", "c2pstr") | ("InterfaceLib", "C2PStr") => {
            PpcImportDispatcherTarget::C2PStr
        }
        ("InterfaceLib", "CopyCStringToPascal") => PpcImportDispatcherTarget::CopyCStringToPascal,
        ("InterfaceLib", "CopyPascalStringToC") => PpcImportDispatcherTarget::CopyPascalStringToC,
        ("InterfaceLib", "__CFStringMakeConstantString") => {
            PpcImportDispatcherTarget::CfStringMakeConstantString
        }
        ("InterfaceLib", "CFStringCreateWithCString") => {
            PpcImportDispatcherTarget::CfStringCreateWithCString
        }
        ("InterfaceLib", "CFStringCreateWithPascalString") => {
            PpcImportDispatcherTarget::CfStringCreateWithPascalString
        }
        ("InterfaceLib", "CFStringCreateWithBytes") => {
            PpcImportDispatcherTarget::CfStringCreateWithBytes
        }
        ("InterfaceLib", "CFStringGetCString") => PpcImportDispatcherTarget::CfStringGetCString,
        ("InterfaceLib", "CFStringGetBytes") => PpcImportDispatcherTarget::CfStringGetBytes,
        ("InterfaceLib", "CFStringGetLength") => PpcImportDispatcherTarget::CfStringGetLength,
        ("InterfaceLib", "CFStringGetSystemEncoding") => {
            PpcImportDispatcherTarget::CfStringGetSystemEncoding
        }
        ("InterfaceLib", "CFRetain") => PpcImportDispatcherTarget::CfRetain,
        ("InterfaceLib", "CFRelease") => PpcImportDispatcherTarget::CfRelease,
        ("InterfaceLib", "CFGetRetainCount") => PpcImportDispatcherTarget::CfGetRetainCount,
        ("InterfaceLib", "CFBundleGetBundleWithIdentifier") => {
            PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier
        }
        ("InterfaceLib", "CFBundleGetMainBundle") => {
            PpcImportDispatcherTarget::CfBundleGetMainBundle
        }
        ("InterfaceLib", "CFBundleCopyPrivateFrameworksURL") => {
            PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl
        }
        ("InterfaceLib", "CFURLCreateCopyAppendingPathComponent") => {
            PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent
        }
        ("InterfaceLib", "CFBundleCreate") => PpcImportDispatcherTarget::CfBundleCreate,
        ("InterfaceLib", "CFBundleLoadExecutable") => {
            PpcImportDispatcherTarget::CfBundleLoadExecutable
        }
        ("InterfaceLib", "UpperText") => PpcImportDispatcherTarget::UpperText,
        // Native Thread Manager exports also live in ThreadsLib.
        // Inside Macintosh: Thread Manager (1999), pp. 15, 62.
        ("InterfaceLib" | "ThreadsLib", "GetCurrentThread" | "MacGetCurrentThread") => {
            PpcImportDispatcherTarget::GetCurrentThread
        }
        ("InterfaceLib", "NewThreadEntryUPP") => PpcImportDispatcherTarget::NewThreadEntryUPP,
        ("InterfaceLib", "DisposeThreadEntryUPP") => {
            PpcImportDispatcherTarget::DisposeThreadEntryUPP
        }
        ("InterfaceLib", "NewThreadTerminationUPP") => {
            PpcImportDispatcherTarget::NewThreadTerminationUPP
        }
        ("InterfaceLib", "DisposeThreadTerminationUPP") => {
            PpcImportDispatcherTarget::DisposeThreadTerminationUPP
        }
        ("InterfaceLib", "NewThreadSwitchUPP") => PpcImportDispatcherTarget::NewThreadSwitchUPP,
        ("InterfaceLib", "DisposeThreadSwitchUPP") => {
            PpcImportDispatcherTarget::DisposeThreadSwitchUPP
        }
        ("InterfaceLib" | "ThreadsLib", "SetThreadTerminator") => {
            PpcImportDispatcherTarget::SetThreadTerminator
        }
        ("InterfaceLib" | "ThreadsLib", "SetThreadSwitcher") => {
            PpcImportDispatcherTarget::SetThreadSwitcher
        }
        ("InterfaceLib" | "ThreadsLib", "GetThreadCurrentTaskRef") => {
            PpcImportDispatcherTarget::GetThreadCurrentTaskRef
        }
        ("InterfaceLib" | "ThreadsLib", "GetThreadStateGivenTaskRef") => {
            PpcImportDispatcherTarget::GetThreadStateGivenTaskRef
        }
        ("InterfaceLib" | "ThreadsLib", "SetThreadReadyGivenTaskRef") => {
            PpcImportDispatcherTarget::SetThreadReadyGivenTaskRef
        }
        ("InterfaceLib" | "ThreadsLib", "GetThreadState") => {
            PpcImportDispatcherTarget::GetThreadState
        }
        ("InterfaceLib" | "ThreadsLib", "SetThreadState") => {
            PpcImportDispatcherTarget::SetThreadState
        }
        ("InterfaceLib" | "ThreadsLib", "SetThreadStateEndCritical") => {
            PpcImportDispatcherTarget::SetThreadStateEndCritical
        }
        ("InterfaceLib" | "ThreadsLib", "CreateThreadPool") => {
            PpcImportDispatcherTarget::CreateThreadPool
        }
        ("InterfaceLib" | "ThreadsLib", "GetFreeThreadCount") => {
            PpcImportDispatcherTarget::GetFreeThreadCount
        }
        ("InterfaceLib" | "ThreadsLib", "GetSpecificFreeThreadCount") => {
            PpcImportDispatcherTarget::GetSpecificFreeThreadCount
        }
        ("InterfaceLib" | "ThreadsLib", "GetDefaultThreadStackSize") => {
            PpcImportDispatcherTarget::GetDefaultThreadStackSize
        }
        ("InterfaceLib" | "ThreadsLib", "ThreadCurrentStackSpace") => {
            PpcImportDispatcherTarget::ThreadCurrentStackSpace
        }
        ("InterfaceLib" | "ThreadsLib", "NewThread") => PpcImportDispatcherTarget::NewThread,
        ("InterfaceLib" | "ThreadsLib", "YieldToThread") => {
            PpcImportDispatcherTarget::YieldToThread
        }
        ("InterfaceLib" | "ThreadsLib", "YieldToAnyThread") => {
            PpcImportDispatcherTarget::YieldToAnyThread
        }
        ("InterfaceLib" | "ThreadsLib", "DisposeThread") => {
            PpcImportDispatcherTarget::DisposeThread
        }
        ("InterfaceLib" | "ThreadsLib", "ThreadBeginCritical") => {
            PpcImportDispatcherTarget::ThreadBeginCritical
        }
        ("InterfaceLib" | "ThreadsLib", "ThreadEndCritical") => {
            PpcImportDispatcherTarget::ThreadEndCritical
        }
        ("InterfaceLib", "GetCurrentProcess" | "GetFrontProcess") => {
            PpcImportDispatcherTarget::GetCurrentProcess
        }
        ("InterfaceLib", "WakeUpProcess") => PpcImportDispatcherTarget::WakeUpProcess,
        ("InterfaceLib", "SameProcess") => PpcImportDispatcherTarget::SameProcess,
        ("InterfaceLib", "GetProcessInformation") => {
            PpcImportDispatcherTarget::GetProcessInformation
        }
        ("InterfaceLib", "ExitToShell") => PpcImportDispatcherTarget::ExitToShell,
        ("InterfaceLib", "SndSoundManagerVersion") => {
            PpcImportDispatcherTarget::SndSoundManagerVersion
        }
        ("SoundLib" | "InterfaceLib", "UnsignedFixedMulDiv" | "UnsignedFixMulDiv") => {
            PpcImportDispatcherTarget::UnsignedFixedMulDiv
        }
        ("SoundLib" | "InterfaceLib", "GetSoundOutputInfo") => {
            PpcImportDispatcherTarget::GetSoundOutputInfo
        }
        ("SoundLib", "GetCompressionInfo") => PpcImportDispatcherTarget::GetCompressionInfo,
        ("InterfaceLib", "GetSoundVol") => PpcImportDispatcherTarget::GetSoundVol,
        ("InterfaceLib", "SetSoundVol") => PpcImportDispatcherTarget::SetSoundVol,
        ("InterfaceLib", "GetDefaultOutputVolume") => {
            PpcImportDispatcherTarget::GetDefaultOutputVolume
        }
        ("InterfaceLib", "SetDefaultOutputVolume") => {
            PpcImportDispatcherTarget::SetDefaultOutputVolume
        }
        ("InterfaceLib", "GetVol") => PpcImportDispatcherTarget::GetVol,
        ("InterfaceLib", "GetWDInfo") => PpcImportDispatcherTarget::GetWDInfo,
        ("InterfaceLib", "OpenWD") => PpcImportDispatcherTarget::OpenWD,
        ("InterfaceLib", "SetVol") => PpcImportDispatcherTarget::SetVol,
        ("InterfaceLib", "HGetVol") => PpcImportDispatcherTarget::HGetVol,
        ("InterfaceLib", "HSetVol") => PpcImportDispatcherTarget::HSetVol,
        ("InterfaceLib", "FlushVol") => PpcImportDispatcherTarget::FlushVol,
        ("InterfaceLib", "PBFlushVol")
        | ("InterfaceLib", "PBFlushVolSync")
        | ("InterfaceLib", "PBFlushVolAsync") => PpcImportDispatcherTarget::PBFlushVol,
        ("InterfaceLib", "SndNewChannel") => PpcImportDispatcherTarget::SndNewChannel,
        ("InterfaceLib", "SetupSndHeader") => PpcImportDispatcherTarget::SetupSndHeader,
        ("InterfaceLib", "SndDisposeChannel") => PpcImportDispatcherTarget::SndDisposeChannel,
        ("InterfaceLib", "SndPlay") => PpcImportDispatcherTarget::SndPlay,
        ("InterfaceLib", "SndChannelStatus") => PpcImportDispatcherTarget::SndChannelStatus,
        ("SoundLib" | "InterfaceLib", "SndGetInfo") => PpcImportDispatcherTarget::SndGetInfo,
        ("SoundLib" | "InterfaceLib", "SndSetInfo") => PpcImportDispatcherTarget::SndSetInfo,
        ("SoundLib", "ParseSndHeader") => PpcImportDispatcherTarget::ParseSndHeader,
        ("InterfaceLib", "SndDoImmediate") => PpcImportDispatcherTarget::SndDoImmediate,
        ("InterfaceLib", "SndDoCommand") => PpcImportDispatcherTarget::SndDoCommand,
        ("InterfaceLib", "SndPlayDoubleBuffer") => PpcImportDispatcherTarget::SndPlayDoubleBuffer,
        ("InterfaceLib", "SndStartFilePlay") => PpcImportDispatcherTarget::SndStartFilePlay,
        ("InterfaceLib", "SndPauseFilePlay") => PpcImportDispatcherTarget::SndPauseFilePlay,
        ("InterfaceLib", "SndStopFilePlay") => PpcImportDispatcherTarget::SndStopFilePlay,
        ("InterfaceLib", "GetSoundHeaderOffset") => PpcImportDispatcherTarget::GetSoundHeaderOffset,
        ("InterfaceLib", "CloseComponent") => PpcImportDispatcherTarget::CloseComponent,
        ("QuickTimeLib", "GetGraphicsImporterForFile") => {
            PpcImportDispatcherTarget::QtGetGraphicsImporterForFile
        }
        ("InterfaceLib", "OpenADefaultComponent") => {
            PpcImportDispatcherTarget::QtOpenADefaultComponent
        }
        ("QuickTimeLib", "GraphicsImportSetDataHandle") => {
            PpcImportDispatcherTarget::QtGraphicsImportSetDataHandle
        }
        ("QuickTimeLib", "GraphicsImportGetImageDescription") => {
            PpcImportDispatcherTarget::QtGraphicsImportGetImageDescription
        }
        ("QuickTimeLib", "GraphicsImportGetBoundsRect") => {
            PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect
        }
        ("QuickTimeLib", "GraphicsImportSetGWorld") => {
            PpcImportDispatcherTarget::QtGraphicsImportSetGWorld
        }
        ("QuickTimeLib", "GraphicsImportDraw") => PpcImportDispatcherTarget::QtGraphicsImportDraw,
        ("QuickTimeLib", "OpenMovieFile") => PpcImportDispatcherTarget::QtOpenMovieFile,
        ("QuickTimeLib", "NewMovieFromFile") => PpcImportDispatcherTarget::QtNewMovieFromFile,
        ("QuickTimeLib", "GetMovieBox") => PpcImportDispatcherTarget::QtGetMovieBox,
        ("QuickTimeLib", "SetMovieBox") => PpcImportDispatcherTarget::QtSetMovieBox,
        ("QuickTimeLib", "SetMovieGWorld") => PpcImportDispatcherTarget::QtSetMovieGWorld,
        ("QuickTimeLib", "StartMovie") => PpcImportDispatcherTarget::QtStartMovie,
        ("QuickTimeLib", "StopMovie") => PpcImportDispatcherTarget::QtStopMovie,
        ("QuickTimeLib", "MoviesTask") => PpcImportDispatcherTarget::QtMoviesTask,
        ("QuickTimeLib", "DisposeMovie") => PpcImportDispatcherTarget::QtDisposeMovie,
        ("QuickTimeLib", "IsMovieDone") => PpcImportDispatcherTarget::QtIsMovieDone,
        ("QuickTimeLib", "GoToBeginningOfMovie") => {
            PpcImportDispatcherTarget::QtGoToBeginningOfMovie
        }
        ("QuickTimeLib", "GoToEndOfMovie") => PpcImportDispatcherTarget::QtGoToEndOfMovie,
        ("QuickTimeLib", "GetMovieDuration") => PpcImportDispatcherTarget::QtGetMovieDuration,
        ("QuickTimeLib", "LoadMovieIntoRam") => PpcImportDispatcherTarget::QtLoadMovieIntoRam,
        ("QuickTimeLib", "CloseMovieFile") => PpcImportDispatcherTarget::QtCloseMovieFile,
        ("QuickTimeLib", "EnterMovies") => PpcImportDispatcherTarget::QtEnterMovies,
        ("QuickTimeLib", "ExitMovies") => PpcImportDispatcherTarget::QtExitMovies,
        ("QuickTimeLib", "GetMoviesError") => PpcImportDispatcherTarget::QtGetMoviesError,
        ("QuickTimeLib", "GetMoviesStickyError") => {
            PpcImportDispatcherTarget::QtGetMoviesStickyError
        }
        ("QuickTimeLib", "ClearMoviesStickyError") => {
            PpcImportDispatcherTarget::QtClearMoviesStickyError
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "ParamText" | "paramtext",
        ) => {
            PpcImportDispatcherTarget::ParamText
        }
        ("InterfaceLib", "X2Fix") => PpcImportDispatcherTarget::X2Fix,
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "Alert",
        ) => {
            PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Alert)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "StopAlert",
        ) => {
            PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Stop)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "NoteAlert",
        ) => {
            PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Note)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CautionAlert",
        ) => {
            PpcImportDispatcherTarget::AlertReturnDefault(crate::dialog_manager::AlertKind::Caution)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "StandardAlert",
        ) => {
            PpcImportDispatcherTarget::StandardAlert
        }
        ("InterfaceLib", "PurgeMem") => PpcImportDispatcherTarget::PurgeMem,
        ("InterfaceLib", "PurgeMemSys") => PpcImportDispatcherTarget::PurgeMemSys,
        ("Apple;Carbon;Networking" | "OpenTransportLib", "NewOTNotifyUPP") => {
            PpcImportDispatcherTarget::NewOTNotifyUPP
        }
        ("InterfaceLib", "ReleaseResource") => PpcImportDispatcherTarget::ReleaseResource,
        ("InterfaceLib", "DetachResource") => PpcImportDispatcherTarget::DetachResource,
        ("InterfaceLib", "ReadPartialResource") => PpcImportDispatcherTarget::ReadPartialResource,
        ("InterfaceLib", "GetIcon") => PpcImportDispatcherTarget::GetIcon,
        ("InterfaceLib", "GetIconSuite") => PpcImportDispatcherTarget::GetIconSuite,
        ("InterfaceLib", "GetPattern") => PpcImportDispatcherTarget::GetPattern,
        ("InterfaceLib", "NewRoutineDescriptor") => PpcImportDispatcherTarget::NewRoutineDescriptor,
        ("InterfaceLib", "NewIOCompletionUPP") => PpcImportDispatcherTarget::NewIOCompletionUPP,
        ("InterfaceLib", "DisposeIOCompletionUPP") => {
            PpcImportDispatcherTarget::DisposeIOCompletionUPP
        }
        ("InterfaceLib", "NewControlUserPaneDrawUPP") => {
            PpcImportDispatcherTarget::NewControlUserPaneDrawUPP
        }
        ("InterfaceLib", "DisposeControlUserPaneDrawUPP") => {
            PpcImportDispatcherTarget::DisposeControlUserPaneDrawUPP
        }
        ("InterfaceLib", "NewAEEventHandlerUPP") => PpcImportDispatcherTarget::NewAEEventHandlerUPP,
        ("InterfaceLib", "DisposeAEEventHandlerUPP") => {
            PpcImportDispatcherTarget::DisposeAEEventHandlerUPP
        }
        ("InterfaceLib", "NewEventHandlerUPP") => {
            PpcImportDispatcherTarget::NewEventHandlerUPP
        }
        ("InterfaceLib", "DisposeEventHandlerUPP") => {
            PpcImportDispatcherTarget::DisposeEventHandlerUPP
        }
        ("InterfaceLib", "NewEventLoopTimerUPP") => {
            PpcImportDispatcherTarget::NewEventLoopTimerUPP
        }
        ("InterfaceLib", "DisposeEventLoopTimerUPP") => {
            PpcImportDispatcherTarget::DisposeEventLoopTimerUPP
        }
        ("InterfaceLib", "NewControlActionUPP") => {
            PpcImportDispatcherTarget::NewControlActionUPP
        }
        ("InterfaceLib", "DisposeControlActionUPP") => {
            PpcImportDispatcherTarget::DisposeControlActionUPP
        }
        ("InterfaceLib", "NewControlKeyFilterUPP") => {
            PpcImportDispatcherTarget::NewControlKeyFilterUPP
        }
        ("InterfaceLib", "DisposeControlKeyFilterUPP") => {
            PpcImportDispatcherTarget::DisposeControlKeyFilterUPP
        }
        ("InterfaceLib", "NewControlEditTextValidationUPP") => {
            PpcImportDispatcherTarget::NewControlEditTextValidationUPP
        }
        ("InterfaceLib", "DisposeControlEditTextValidationUPP") => {
            PpcImportDispatcherTarget::DisposeControlEditTextValidationUPP
        }
        ("InterfaceLib", "NewFatRoutineDescriptor") => {
            PpcImportDispatcherTarget::NewFatRoutineDescriptor
        }
        ("InterfaceLib", "DisposeRoutineDescriptor") => {
            PpcImportDispatcherTarget::DisposeRoutineDescriptor
        }
        ("InterfaceLib", "CallUniversalProc") => PpcImportDispatcherTarget::CallUniversalProc,
        ("InterfaceLib", "CallOSTrapUniversalProc") => {
            PpcImportDispatcherTarget::CallOSTrapUniversalProc
        }
        ("InterfaceLib", "NGetTrapAddress") => PpcImportDispatcherTarget::NGetTrapAddress,
        ("InterfaceLib", "GetToolTrapAddress") | ("InterfaceLib", "GetToolboxTrapAddress") => {
            PpcImportDispatcherTarget::GetToolTrapAddress
        }
        ("InterfaceLib", "GetOSTrapAddress") => PpcImportDispatcherTarget::GetOSTrapAddress,
        ("InterfaceLib", "SetToolTrapAddress") | ("InterfaceLib", "SetToolboxTrapAddress") => {
            PpcImportDispatcherTarget::SetToolTrapAddress
        }
        ("InterfaceLib", "SetOSTrapAddress") => PpcImportDispatcherTarget::SetOSTrapAddress,
        ("InterfaceLib", "NSetTrapAddress") => PpcImportDispatcherTarget::NSetTrapAddress,
        ("InterfaceLib", "LMGetCurrentA5") => PpcImportDispatcherTarget::LMGetCurrentA5,
        ("InterfaceLib", "InsTime") => PpcImportDispatcherTarget::InsTime,
        ("InterfaceLib", "InsXTime") => PpcImportDispatcherTarget::InsXTime,
        ("InterfaceLib", "PrimeTime") => PpcImportDispatcherTarget::PrimeTime,
        ("InterfaceLib", "RmvTime") => PpcImportDispatcherTarget::RmvTime,
        ("InterfaceLib", "VInstall") => PpcImportDispatcherTarget::VInstall,
        ("InterfaceLib", "VRemove") => PpcImportDispatcherTarget::VRemove,
        ("InterfaceLib", "SlotVInstall") => PpcImportDispatcherTarget::SlotVInstall,
        ("InterfaceLib", "SlotVRemove") => PpcImportDispatcherTarget::SlotVRemove,
        ("InterfaceLib", "BitClr") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::BitClear,
        ),
        ("InterfaceLib", "BitNot") => {
            PpcImportDispatcherTarget::LegacyMemoryUtility(PpcLegacyMemoryUtilityOperation::BitNot)
        }
        ("InterfaceLib", "BitSet") => {
            PpcImportDispatcherTarget::LegacyMemoryUtility(PpcLegacyMemoryUtilityOperation::BitSet)
        }
        ("InterfaceLib", "Fix2X") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::FixToExtended,
        ),
        ("InterfaceLib", "GetMyZone") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::GetMyZone,
        ),
        ("InterfaceLib", "HandleZone") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::HandleZone,
        ),
        ("InterfaceLib", "LockMemory") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::LockMemory,
        ),
        ("InterfaceLib", "MaxBlock") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::MaxBlock,
        ),
        ("InterfaceLib", "PurgeSpace") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::PurgeSpace,
        ),
        ("InterfaceLib", "ReserveMem") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::ReserveMem,
        ),
        ("InterfaceLib", "SetGrowZone") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::SetGrowZone,
        ),
        ("InterfaceLib", "StackSpace") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::StackSpace,
        ),
        ("InterfaceLib", "TempFreeMem") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::TempFreeMem,
        ),
        ("InterfaceLib", "UnlockMemory") => PpcImportDispatcherTarget::LegacyMemoryUtility(
            PpcLegacyMemoryUtilityOperation::UnlockMemory,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "AutoEmbedControl" | "autoembedcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::AutoEmbedControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ChangeControlPropertyAttributes" | "changecontrolpropertyattributes",
        ) => PpcImportDispatcherTarget::LegacyControl(
            PpcLegacyControlOperation::ChangeControlPropertyAttributes,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CountSubControls" | "countsubcontrols",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::CountSubControls),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CreateRootControl" | "createrootcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::CreateRootControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DisposeControl" | "disposecontrol" | "DisposControl" | "disposcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisposeControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "Draw1Control" | "draw1control" | "DrawOneControl" | "drawonecontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DrawOneControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "EmbedControl" | "embedcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::EmbedControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "FindControl" | "findcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::FindControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlMaximum"
                | "getcontrolmaximum"
                | "GetCtlMax"
                | "getctlmax"
                | "GetControlMax"
                | "getcontrolmax",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMaximum),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlAction" | "getcontrolaction",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlAction),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlReference" | "getcontrolreference" | "GetCRefCon" | "getcrefcon",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlReference),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlMinimum"
                | "getcontrolminimum"
                | "GetCtlMin"
                | "getctlmin"
                | "GetControlMin"
                | "getcontrolmin",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlMinimum),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlTitle" | "getcontroltitle" | "GetCTitle" | "getctitle",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlTitle),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlValue" | "getcontrolvalue" | "GetCtlValue" | "getctlvalue",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlValue),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlVariant"
                | "getcontrolvariant"
                | "GetCVariant"
                | "getcvariant",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlVariant),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlProperty" | "getcontrolproperty",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetControlProperty),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlPropertyAttributes" | "getcontrolpropertyattributes",
        ) => PpcImportDispatcherTarget::LegacyControl(
            PpcLegacyControlOperation::GetControlPropertyAttributes,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetControlPropertySize" | "getcontrolpropertysize",
        ) => PpcImportDispatcherTarget::LegacyControl(
            PpcLegacyControlOperation::GetControlPropertySize,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetIndexedSubControl" | "getindexedsubcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(
            PpcLegacyControlOperation::GetIndexedSubControl,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetNewControl" | "getnewcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetNewControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetRootControl" | "getrootcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetRootControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetSuperControl" | "getsupercontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::GetSuperControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HideControl" | "hidecontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::HideControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "KillControls" | "killcontrols",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::KillControls),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "MoveControl" | "movecontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::MoveControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "NewControl" | "newcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::NewControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "RemoveControlProperty" | "removecontrolproperty",
        ) => PpcImportDispatcherTarget::LegacyControl(
            PpcLegacyControlOperation::RemoveControlProperty,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlMaximum"
                | "setcontrolmaximum"
                | "SetCtlMax"
                | "setctlmax"
                | "SetControlMax"
                | "setcontrolmax",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMaximum),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlAction" | "setcontrolaction",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlAction),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlReference" | "setcontrolreference" | "SetCRefCon" | "setcrefcon",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlReference),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlMinimum"
                | "setcontrolminimum"
                | "SetCtlMin"
                | "setctlmin"
                | "SetControlMin"
                | "setcontrolmin",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlMinimum),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlProperty" | "setcontrolproperty",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SetControlProperty),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetControlSupervisor" | "setcontrolsupervisor",
        ) => PpcImportDispatcherTarget::LegacyControl(
            PpcLegacyControlOperation::SetControlSupervisor,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ShowControl" | "showcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::ShowControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SizeControl" | "sizecontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::SizeControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "TestControl" | "testcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TestControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "TrackControl" | "trackcontrol",
        ) => PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::TrackControl),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ActiveNonFloatingWindow" | "activenonfloatingwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ActiveNonFloatingWindow,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "BringToFront" | "bringtofront",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::BringToFront)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CalcVis" | "calcvis",
        ) => PpcImportDispatcherTarget::LegacyWindow(
            PpcLegacyWindowOperation::CalculateVisibleRegion,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ChangeWindowAttributes" | "changewindowattributes",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ChangeWindowAttributes,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ChangeWindowPropertyAttributes" | "changewindowpropertyattributes",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ChangeWindowPropertyAttributes,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CheckUpdate" | "checkupdate",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CheckUpdate)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "CreateNewWindow" | "createnewwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CreateNewWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DisposeWindow" | "disposewindow" | "DisposWindow" | "disposwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "DragWindow" | "dragwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DragWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetNewWindow" | "getnewwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetNewWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetNextWindow" | "getnextwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetNextWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetPreviousWindow" | "getpreviouswindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetPreviousWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetUserFocusWindow" | "getuserfocuswindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetUserFocusWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowAttributes" | "getwindowattributes",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowAttributes,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowBounds" | "getwindowbounds",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowBounds)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowCancelButton" | "getwindowcancelbutton",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowCancelButton)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowContentRgn" | "getwindowcontentrgn",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowContentRgn)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowDefProc" | "getwindowdefproc",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowDefProc)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowDefaultButton" | "getwindowdefaultbutton",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowDefaultButton)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowFeatures" | "getwindowfeatures",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFeatures)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowFromPort" | "getwindowfromport",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFromPort)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowGoAwayFlag" | "getwindowgoawayflag",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowGoAwayFlag)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowGreatestArea" | "getwindowgreatestarea",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowGreatestArea,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowIdealUserState" | "getwindowidealuserstate",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowIdealUserState)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowKind" | "getwindowkind",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowKind)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowModality" | "getwindowmodality",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowModality)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowPortBounds" | "getwindowportbounds",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowPortBounds,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowProperty" | "getwindowproperty",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProperty)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowPropertyAttributes" | "getwindowpropertyattributes",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowPropertyAttributes,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowPropertySize" | "getwindowpropertysize",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowPropertySize,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowProxyIcon" | "getwindowproxyicon",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProxyIcon)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowRegion" | "getwindowregion",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowRegion)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowSpareFlag" | "getwindowspareflag",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowSpareFlag)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowStandardState" | "getwindowstandardstate",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStandardState)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowStructureRgn" | "getwindowstructurergn",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowStructureRgn,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowStructureWidths" | "getwindowstructurewidths",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStructureWidths)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWTitle" | "getwtitle" | "GetWindowTitle" | "getwindowtitle",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowUpdateRgn" | "getwindowupdatergn",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowUpdateRgn)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetWindowUserState" | "getwindowuserstate",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowUserState)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GrowWindow" | "growwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GrowWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "HiliteWindow" | "hilitewindow" | "HighlightWindow" | "highlightwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::HighlightWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InvalWindowRect" | "invalwindowrect",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::InvalWindowRect,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "InvalWindowRgn" | "invalwindowrgn",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::InvalWindowRgn,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsWindowActive" | "iswindowactive",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowActive)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsWindowHilited" | "iswindowhilited",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowHilited)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsWindowModified" | "iswindowmodified",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowModified)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsWindowPathSelectClick" | "iswindowpathselectclick",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowPathSelectClick)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsWindowUpdatePending" | "iswindowupdatepending",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::IsWindowUpdatePending,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "IsWindowVisible" | "iswindowvisible",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowVisible)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "NewWindow" | "newwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::NewWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "RemoveWindowProperty" | "removewindowproperty",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::RemoveWindowProperty,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "RemoveWindowProxy" | "removewindowproxy",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RemoveWindowProxy)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ReshapeCustomWindow" | "reshapecustomwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ReshapeCustomWindow,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "RepositionWindow" | "repositionwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RepositionWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SendBehind" | "sendbehind",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SendBehind)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetUserFocusWindow" | "setuserfocuswindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetUserFocusWindow)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowBounds" | "setwindowbounds",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowBounds)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowCancelButton" | "setwindowcancelbutton",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowCancelButton)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowDefaultButton" | "setwindowdefaultbutton",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowDefaultButton)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowIdealUserState" | "setwindowidealuserstate",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowIdealUserState)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowKind" | "setwindowkind",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowKind)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowModality" | "setwindowmodality",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModality)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowModified" | "setwindowmodified",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModified)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowProperty" | "setwindowproperty",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProperty)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowProxyIcon" | "setwindowproxyicon",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProxyIcon)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowStandardState" | "setwindowstandardstate",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowStandardState)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWTitle" | "setwtitle" | "SetWindowTitle" | "setwindowtitle",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetWindowUserState" | "setwindowuserstate",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowUserState)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "TrackBox" | "trackbox",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::TrackBox)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "TrackGoAway" | "trackgoaway",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::TrackGoAway)
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ValidWindowRect" | "validwindowrect",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ValidWindowRect,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ValidWindowRgn" | "validwindowrgn",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ValidWindowRgn,
            )
        }
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "ZoomWindow" | "zoomwindow",
        ) => {
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ZoomWindow)
        }
        ("InterfaceLib", "AECountItems") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CountItems,
        ),
        ("InterfaceLib", "AECreateAppleEvent") => {
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::CreateAppleEvent,
            )
        }
        ("InterfaceLib", "AECreateDesc") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
        ("InterfaceLib", "AECreateList") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
        ("InterfaceLib", "AEPutDesc") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutDesc,
        ),
        ("InterfaceLib", "AEDisposeDesc") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::DisposeDesc,
        ),
        ("InterfaceLib", "AEGetAttributePtr") => {
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::GetAttributePtr,
            )
        }
        ("InterfaceLib", "AEGetNthPtr") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthPtr,
        ),
        ("InterfaceLib", "AEGetParamDesc") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
        ("InterfaceLib", "AEGetParamPtr") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
        ("InterfaceLib", "AESizeOfParam") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::SizeOfParam,
        ),
        ("InterfaceLib", "AEPutParamDesc") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamDesc,
        ),
        ("InterfaceLib", "AEPutParamPtr") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
        ("InterfaceLib", "AESend") => PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "AppendDITL" | "AppendDitl",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AppendDitl,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "AutoPositionDialog"
                | "autopositiondialog"
                | "PositionDialog"
                | "positiondialog",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::AutoPositionDialog,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CloseStandardSheet" | "closestandardsheet",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CloseStandardSheet,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CountDITL" | "CountDitl",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CountDitl,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CreateStandardAlert" | "createstandardalert",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CreateStandardAlert,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "CreateStandardSheet" | "createstandardsheet",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::CreateStandardSheet,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "DialogSelect",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::DialogSelect,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "FindDialogItem" | "FindDItem",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::FindDialogItem,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "FlashDialogControl" | "flashdialogcontrol",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::FlashDialogControl,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogFilter" | "getdialogfilter",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogFilter,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogItemInit" | "getdialogiteminit",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogItemInit,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogKeyboardFocusItem" | "getdialogkeyboardfocusitem",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogKeyboardFocusItem,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogTextEditHandle" | "getdialogtextedithandle",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTextEditHandle,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogTimeout" | "getdialogtimeout",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTimeout,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetDialogTracksCursor" | "getdialogtrackscursor",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetDialogTracksCursor,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetModalDialogEventMask" | "getmodaldialogeventmask",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetModalDialogEventMask,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetParamText" | "getparamtext",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetParamText,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetSheetWindowParent" | "getsheetwindowparent",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetSheetWindowParent,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "GetStandardAlertDefaultParams" | "getstandardalertdefaultparams",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::GetStandardAlertDefaultParams,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "HideDialogItem" | "HideDItem",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::HideDialogItem,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "HideSheetWindow" | "hidesheetwindow",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::HideSheetWindow,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "InsertDialogItem" | "insertdialogitem",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::InsertDialogItem,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "IsDialogEvent",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::IsDialogEvent,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "IsDialogTracksCursor" | "isdialogtrackscursor",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::IsDialogTracksCursor,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "RemoveDialogItems" | "removedialogitems",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::RemoveDialogItems,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "RunStandardAlert" | "runstandardalert",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::RunStandardAlert,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogFilter" | "setdialogfilter",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogFilter,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogKeyboardFocusItem"
            | "setdialogkeyboardfocusitem"
            | "SetDialogKeyboardFocus"
            | "setdialogkeyboardfocus",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogKeyboardFocusItem,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetDialogTimeout" | "setdialogtimeout",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetDialogTimeout,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "SetModalDialogEventMask" | "setmodaldialogeventmask",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::SetModalDialogEventMask,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "ShortenDITL" | "ShortenDitl",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::ShortenDitl,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "ShowDialogItem" | "ShowDItem",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::ShowDialogItem,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "ShowSheetWindow" | "showsheetwindow",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::ShowSheetWindow,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "UpdateDialog" | "UpdtDialog",
        ) => PpcImportDispatcherTarget::DialogCompatibility(
            PpcDialogCompatibilityOperation::UpdateDialog,
        ),
        ("InterfaceLib", "AnimateEntry") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::AnimateEntry,
        ),
        ("InterfaceLib", "AnimatePalette") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::AnimatePalette,
        ),
        ("InterfaceLib", "BackPat") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::BackPat,
        ),
        ("InterfaceLib", "BackPixPat") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::BackPixPat,
        ),
        ("InterfaceLib", "ClosePicture") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::ClosePicture,
        ),
        ("InterfaceLib", "CopyDeepMask") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::CopyDeepMask,
        ),
        ("InterfaceLib", "CopyMask") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::CopyMask,
        ),
        ("InterfaceLib", "CopyPalette") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::CopyPalette,
        ),
        ("InterfaceLib", "CTab2Palette") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::Ctab2Palette,
        ),
        ("InterfaceLib", "DisposeGDevice") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::DisposeGDevice,
        ),
        ("InterfaceLib", "DisposePalette") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::DisposePalette,
        ),
        ("InterfaceLib", "DisposePixPat" | "DisposPixPat") => {
            PpcImportDispatcherTarget::QuickDrawCompatibility(
                PpcQuickDrawCompatibilityOperation::DisposePixPat,
            )
        }
        ("InterfaceLib", "Exp1to3") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::Exp1To3,
        ),
        ("InterfaceLib", "Exp1to6") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::Exp1To6,
        ),
        ("InterfaceLib", "GetCPixel") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::GetCPixel,
        ),
        ("InterfaceLib", "GetEntryColor") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::GetEntryColor,
        ),
        ("InterfaceLib", "GetEntryUsage") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::GetEntryUsage,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetItemIcon" | "getitemicon" | "GetMenuItemIcon" | "getmenuitemicon",
        ) => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::GetItemIcon,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "GetItemStyle" | "getitemstyle" | "GetMenuItemStyle" | "getmenuitemstyle",
        ) => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::GetItemStyle,
        ),
        ("InterfaceLib", "GetNewPalette") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::GetNewPalette,
        ),
        ("InterfaceLib", "NewGDevice") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewGDevice,
        ),
        ("InterfaceLib", "NewPalette") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewPalette,
        ),
        ("InterfaceLib", "NewPixPat") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::NewPixPat,
        ),
        ("InterfaceLib", "OpenPicture") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::OpenPicture,
        ),
        ("InterfaceLib", "Palette2CTab") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::Palette2Ctab,
        ),
        ("InterfaceLib", "PenPat") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::PenPat,
        ),
        ("InterfaceLib", "PlotIcon") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::PlotIcon,
        ),
        ("InterfaceLib", "ScrollRect") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::ScrollRect,
        ),
        ("InterfaceLib", "SetCPixel") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::SetCPixel,
        ),
        ("InterfaceLib", "SetEntryColor") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::SetEntryColor,
        ),
        ("InterfaceLib", "SetEntryUsage") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::SetEntryUsage,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetItemIcon" | "setitemicon" | "SetMenuItemIcon" | "setmenuitemicon",
        ) => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::SetItemIcon,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "CarbonLib",
            "SetItemStyle" | "setitemstyle" | "SetMenuItemStyle" | "setmenuitemstyle",
        ) => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::SetItemStyle,
        ),
        ("InterfaceLib", "SetStdCProcs") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::SetStdCProcs,
        ),
        ("InterfaceLib", "SetStdProcs") => PpcImportDispatcherTarget::QuickDrawCompatibility(
            PpcQuickDrawCompatibilityOperation::SetStdProcs,
        ),
        ("InterfaceLib", "BuildDDPwds") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::BuildDdPwds,
        ),
        ("InterfaceLib", "CTBGetCTBVersion") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::CtbGetCtbVersion,
        ),
        ("InterfaceLib", "CallComponentUPP") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::CallComponentUpp,
        ),
        ("InterfaceLib", "DIBadMount") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::DiBadMount,
        ),
        ("InterfaceLib", "DILoad") => {
            PpcImportDispatcherTarget::SystemCompatibility(PpcSystemCompatibilityOperation::DiLoad)
        }
        ("InterfaceLib", "DIUnload") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::DiUnload,
        ),
        ("InterfaceLib", "Debugger") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::Debugger,
        ),
        ("InterfaceLib", "Dequeue") => {
            PpcImportDispatcherTarget::SystemCompatibility(PpcSystemCompatibilityOperation::Dequeue)
        }
        ("InterfaceLib", "Enqueue") => {
            PpcImportDispatcherTarget::SystemCompatibility(PpcSystemCompatibilityOperation::Enqueue)
        }
        ("InterfaceLib", "FindNextComponent") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::FindNextComponent,
        ),
        ("InterfaceLib", "GetNextProcess") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetNextProcess,
        ),
        ("InterfaceLib", "GetScript") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetScript,
        ),
        ("InterfaceLib", "GetScriptManagerVariable") => {
            PpcImportDispatcherTarget::SystemCompatibility(
                PpcSystemCompatibilityOperation::GetScriptManagerVariable,
            )
        }
        ("InterfaceLib", "GetScriptVariable") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetScriptVariable,
        ),
        ("InterfaceLib", "IntlScript") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::IntlScript,
        ),
        ("InterfaceLib", "GetSysBeepVolume") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::GetSysBeepVolume,
        ),
        ("InterfaceLib", "IUCompString") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::IuCompString,
        ),
        ("InterfaceLib", "IUDateString") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::IuDateString,
        ),
        ("InterfaceLib", "InitCRM") => {
            PpcImportDispatcherTarget::SystemCompatibility(PpcSystemCompatibilityOperation::InitCrm)
        }
        ("InterfaceLib", "InitCTBUtilities") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::InitCtbUtilities,
        ),
        ("InterfaceLib", "KeyTranslate") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::KeyTranslate,
        ),
        ("InterfaceLib", "LMGetCurApName") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LmGetCurApName,
        ),
        ("InterfaceLib", "LMGetSysFontFam") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LmGetSysFontFam,
        ),
        ("InterfaceLib", "LMGetSysFontSize") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LmGetSysFontSize,
        ),
        ("InterfaceLib", "LaunchApplication") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LaunchApplication,
        ),
        ("InterfaceLib", "MIDIAddPort") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::MidiAddPort,
        ),
        ("InterfaceLib", "MIDIRemovePort") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::MidiRemovePort,
        ),
        ("InterfaceLib", "MIDISignOut") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::MidiSignOut,
        ),
        ("InterfaceLib", "MIDIWritePacket") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::MidiWritePacket,
        ),
        ("InterfaceLib", "Munger") => {
            PpcImportDispatcherTarget::SystemCompatibility(PpcSystemCompatibilityOperation::Munger)
        }
        ("InterfaceLib", "NMRemove") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::NmRemove,
        ),
        ("InterfaceLib", "ObscureCursor") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::ObscureCursor,
        ),
        ("InterfaceLib", "OpenDefaultComponent") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::OpenDefaultComponent,
        ),
        (
            "InterfaceLib" | "AppearanceLib" | "DialogsLib" | "CarbonLib",
            "ResetAlertStage",
        ) => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::ResetAlertStage,
        ),
        ("InterfaceLib", "SetFrontProcess") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::SetFrontProcess,
        ),
        ("InterfaceLib", "StyledLineBreak") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::StyledLineBreak,
        ),
        ("InterfaceLib", "SystemEdit") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::SystemEdit,
        ),
        ("InterfaceLib", "TruncText") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::TruncText,
        ),
        ("InterfaceLib", "UpperString") => PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::UpperString,
        ),
        ("InterfaceLib", "OpenDF") => {
            PpcImportDispatcherTarget::FileCompatibility(PpcFileCompatibilityOperation::OpenDf)
        }
        ("InterfaceLib", "OpenRF") => {
            PpcImportDispatcherTarget::FileCompatibility(PpcFileCompatibilityOperation::OpenRf)
        }
        ("InterfaceLib", "PBCatSearchSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbCatSearchSync,
        ),
        ("InterfaceLib", "PBCloseWDSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbCloseWdSync,
        ),
        ("InterfaceLib", "PBDirCreateSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbDirCreateSync,
        ),
        ("InterfaceLib", "PBGetFPosSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbGetFPosSync,
        ),
        ("InterfaceLib", "PBGetWDInfoSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbGetWdInfoSync,
        ),
        ("InterfaceLib", "PBHGetVolParmsSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbHGetVolParmsSync,
        ),
        ("InterfaceLib", "PBHGetVolSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbHGetVolSync,
        ),
        ("InterfaceLib", "PBHOpenRFSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbHOpenRfSync,
        ),
        ("InterfaceLib", "PBHSetVolSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbHSetVolSync,
        ),
        ("InterfaceLib", "PBOpenWDSync") => PpcImportDispatcherTarget::FileCompatibility(
            PpcFileCompatibilityOperation::PbOpenWdSync,
        ),
        ("InterfaceLib", "create") => {
            PpcImportDispatcherTarget::FileCompatibility(PpcFileCompatibilityOperation::Create)
        }
        ("InterfaceLib", "fsopen") => {
            PpcImportDispatcherTarget::FileCompatibility(PpcFileCompatibilityOperation::FsOpen)
        }
        ("InterfaceLib", "GetBridgeAddress") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::GetBridgeAddress,
        ),
        ("InterfaceLib", "GetNodeAddress") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::GetNodeAddress,
        ),
        ("InterfaceLib", "GetZoneList") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::GetZoneList,
        ),
        ("InterfaceLib", "MPPOpen") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::MppOpen,
        ),
        ("InterfaceLib", "NBPExtract") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::NbpExtract,
        ),
        ("InterfaceLib", "NBPSetEntity") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::NbpSetEntity,
        ),
        ("InterfaceLib", "NBPSetNTE") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::NbpSetNte,
        ),
        ("InterfaceLib", "PCloseSkt") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::PCloseSkt,
        ),
        ("InterfaceLib", "PKillNBP") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::PKillNbp,
        ),
        ("InterfaceLib", "PLookupName") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::PLookupName,
        ),
        ("InterfaceLib", "POpenSkt") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::POpenSkt,
        ),
        ("InterfaceLib", "PRegisterName") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::PRegisterName,
        ),
        ("InterfaceLib", "PRemoveName") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::PRemoveName,
        ),
        ("InterfaceLib", "PSetSelfSend") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::PSetSelfSend,
        ),
        ("InterfaceLib", "PWriteDDP") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::PWriteDdp,
        ),
        ("InterfaceLib", "StandardNBP") => PpcImportDispatcherTarget::AppleTalkCompatibility(
            PpcAppleTalkCompatibilityOperation::StandardNbp,
        ),
        ("InterfaceLib", "PrClose") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrClose,
        ),
        ("InterfaceLib", "PrCloseDoc") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrCloseDoc,
        ),
        ("InterfaceLib", "PrClosePage") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrClosePage,
        ),
        ("InterfaceLib", "PrError") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrError,
        ),
        ("InterfaceLib", "PrJobDialog") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrJobDialog,
        ),
        ("InterfaceLib", "PrOpen") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrOpen,
        ),
        ("InterfaceLib", "PrOpenDoc") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrOpenDoc,
        ),
        ("InterfaceLib", "PrOpenPage") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrOpenPage,
        ),
        ("InterfaceLib", "PrPicFile") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrPicFile,
        ),
        ("InterfaceLib", "PrStlDialog") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrStlDialog,
        ),
        ("InterfaceLib", "PrintDefault") => PpcImportDispatcherTarget::PrintingCompatibility(
            PpcPrintingCompatibilityOperation::PrintDefault,
        ),
        (
            "InterfaceLib",
            "SFindStruct" | "SGetBlock" | "SGetCString" | "SGetSRsrc" | "SGetTypeSRsrc"
            | "SNextTypeSRsrc",
        ) => PpcImportDispatcherTarget::SlotCompatibility,
        ("InterfaceLib", "CustomGetFile") => PpcImportDispatcherTarget::StandardFileCompatibility(
            PpcStandardFileOperation::CustomGetFile,
        ),
        ("InterfaceLib", "CustomPutFile") => PpcImportDispatcherTarget::StandardFileCompatibility(
            PpcStandardFileOperation::CustomPutFile,
        ),
        ("InterfaceLib", "SFGetFile") => PpcImportDispatcherTarget::StandardFileCompatibility(
            PpcStandardFileOperation::SfGetFile,
        ),
        ("InterfaceLib", "SFPGetFile") => PpcImportDispatcherTarget::StandardFileCompatibility(
            PpcStandardFileOperation::SfpGetFile,
        ),
        ("InterfaceLib", "SFPPutFile") => PpcImportDispatcherTarget::StandardFileCompatibility(
            PpcStandardFileOperation::SfpPutFile,
        ),
        ("InterfaceLib", "SFPutFile") => PpcImportDispatcherTarget::StandardFileCompatibility(
            PpcStandardFileOperation::SfPutFile,
        ),
        ("InterfaceLib", "StandardPutFile") => {
            PpcImportDispatcherTarget::StandardFileCompatibility(
                PpcStandardFileOperation::StandardPutFile,
            )
        }
        ("InterfaceLib", "SPBCloseDevice") => PpcImportDispatcherTarget::SoundInputCompatibility(
            PpcSoundInputCompatibilityOperation::CloseDevice,
        ),
        ("InterfaceLib", "SPBGetDeviceInfo") => PpcImportDispatcherTarget::SoundInputCompatibility(
            PpcSoundInputCompatibilityOperation::GetDeviceInfo,
        ),
        ("InterfaceLib", "SPBOpenDevice") => PpcImportDispatcherTarget::SoundInputCompatibility(
            PpcSoundInputCompatibilityOperation::OpenDevice,
        ),
        ("InterfaceLib", "SPBRecord") => PpcImportDispatcherTarget::SoundInputCompatibility(
            PpcSoundInputCompatibilityOperation::Record,
        ),
        ("InterfaceLib", "SPBSetDeviceInfo") => PpcImportDispatcherTarget::SoundInputCompatibility(
            PpcSoundInputCompatibilityOperation::SetDeviceInfo,
        ),
        ("InterfaceLib", "SPBStopRecording") => PpcImportDispatcherTarget::SoundInputCompatibility(
            PpcSoundInputCompatibilityOperation::StopRecording,
        ),
        ("SpeechLib", "CountVoices") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::CountVoices,
        ),
        ("SpeechLib", "DisposeSpeechChannel") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::DisposeSpeechChannel,
        ),
        ("SpeechLib", "GetIndVoice") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::GetIndVoice,
        ),
        ("SpeechLib", "GetVoiceDescription") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::GetVoiceDescription,
        ),
        ("SpeechLib", "NewSpeechChannel") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::NewSpeechChannel,
        ),
        ("SpeechLib", "SpeakString") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::SpeakString,
        ),
        ("SpeechLib", "SpeakText") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::SpeakText,
        ),
        ("SpeechLib", "SpeechBusy") => PpcImportDispatcherTarget::SpeechCompatibility(
            PpcSpeechCompatibilityOperation::SpeechBusy,
        ),
        ("QuickTimeLib", "GetMovieActive") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMovieActive,
        ),
        ("QuickTimeLib", "GetMovieTimeBase") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMovieTimeBase,
        ),
        ("QuickTimeLib", "GetMovieTimeScale") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMovieTimeScale,
        ),
        ("QuickTimeLib", "GetMoviePreferredRate") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMoviePreferredRate,
        ),
        ("QuickTimeLib", "GetMoviePreferredVolume") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMoviePreferredVolume,
        ),
        ("QuickTimeLib", "GetMovieIndTrackType") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMovieIndTrackType,
        ),
        ("QuickTimeLib", "GetMovieUserData") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMovieUserData,
        ),
        ("QuickTimeLib", "GetUserData") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetUserData,
        ),
        ("QuickTimeLib", "GetTrackMedia") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetTrackMedia,
        ),
        ("QuickTimeLib", "GetMediaSampleDescription") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMediaSampleDescription,
        ),
        ("QuickTimeLib", "MCDoAction") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::MCDoAction,
        ),
        ("QuickTimeLib", "NewMovieController") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::NewMovieController,
        ),
        ("QuickTimeLib", "DisposeMovieController") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::DisposeMovieController,
        ),
        ("QuickTimeLib", "MCSetMovie") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::MCSetMovie,
        ),
        ("QuickTimeLib", "MCGetControllerBoundsRect") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::MCGetControllerBoundsRect,
        ),
        ("QuickTimeLib", "MCSetActionFilterWithRefCon") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::MCSetActionFilterWithRefCon,
        ),
        ("QuickTimeLib", "MCSetControllerBoundsRect") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::MCSetControllerBoundsRect,
        ),
        ("QuickTimeLib", "MCIdle") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::MCIdle,
        ),
        ("QuickTimeLib", "MCGetCurrentTime") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::MCGetCurrentTime,
        ),
        ("QuickTimeLib", "GetMovieVolume") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::GetMovieVolume,
        ),
        ("QuickTimeLib", "NewMovieFromDataFork") => {
            PpcImportDispatcherTarget::QuickTimeCompatibility(
                PpcQuickTimeCompatibilityOperation::NewMovieFromDataFork,
            )
        }
        ("QuickTimeLib", "PrerollMovie") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::PrerollMovie,
        ),
        ("QuickTimeLib", "SetMovieActive") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::SetMovieActive,
        ),
        ("QuickTimeLib", "SetMovieVolume") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::SetMovieVolume,
        ),
        ("QuickTimeLib", "SetTimeBaseFlags") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::SetTimeBaseFlags,
        ),
        ("QuickTimeLib", "UpdateMovie") => PpcImportDispatcherTarget::QuickTimeCompatibility(
            PpcQuickTimeCompatibilityOperation::UpdateMovie,
        ),
        ("InputSprocketLib", "ISpDevices_ActivateClass") => {
            PpcImportDispatcherTarget::InputSprocketCompatibility(
                PpcInputSprocketCompatibilityOperation::DevicesActivateClass,
            )
        }
        ("InputSprocketLib", "ISpDevices_DeactivateClass") => {
            PpcImportDispatcherTarget::InputSprocketCompatibility(
                PpcInputSprocketCompatibilityOperation::DevicesDeactivateClass,
            )
        }
        ("InputSprocketLib", "ISpElement_DisposeVirtual") => {
            PpcImportDispatcherTarget::InputSprocketCompatibility(
                PpcInputSprocketCompatibilityOperation::ElementDisposeVirtual,
            )
        }
        ("InputSprocketLib", "ISpElement_Flush") => {
            PpcImportDispatcherTarget::InputSprocketCompatibility(
                PpcInputSprocketCompatibilityOperation::ElementFlush,
            )
        }
        ("InputSprocketLib", "ISpElement_GetNextEvent") => {
            PpcImportDispatcherTarget::InputSprocketCompatibility(
                PpcInputSprocketCompatibilityOperation::ElementGetNextEvent,
            )
        }
        ("InputSprocketLib", "ISpTickle") => PpcImportDispatcherTarget::InputSprocketCompatibility(
            PpcInputSprocketCompatibilityOperation::Tickle,
        ),
        ("MathLib", "dec2num") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Dec2Num)
        }
        ("MathLib", "dec2str") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Dec2Str)
        }
        ("MathLib", "feclearexcept") => PpcImportDispatcherTarget::MathCompatibility(
            PpcMathCompatibilityOperation::FeClearExcept,
        ),
        ("MathLib", "fetestexcept") => PpcImportDispatcherTarget::MathCompatibility(
            PpcMathCompatibilityOperation::FeTestExcept,
        ),
        ("MathLib", "floor") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Floor)
        }
        ("MathLib", "ldexp") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Ldexp)
        }
        ("MathLib", "ldtox80") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::LdToX80)
        }
        ("MathLib", "modf") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Modf)
        }
        ("MathLib", "num2dec") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Num2Dec)
        }
        ("MathLib", "str2dec") => {
            PpcImportDispatcherTarget::MathCompatibility(PpcMathCompatibilityOperation::Str2Dec)
        }
        ("StdCLib", "qsort") => {
            PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Qsort)
        }
        ("StdCLib", "signal") => {
            PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Signal)
        }
        ("StdCLib", "sscanf") => {
            PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Sscanf)
        }
        ("StdCLib", "strftime") => {
            PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Strftime)
        }
        ("StdCLib", "vsprintf") => {
            PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Vsprintf)
        }
        ("ObjectSupportLib", "AEObjectInit") => PpcImportDispatcherTarget::ObjectSupportInit,
        ("ObjectSupportLib", "AEInstallObjectAccessor") => {
            PpcImportDispatcherTarget::ObjectSupportInstallAccessor
        }
        ("ObjectSupportLib", "AEGetObjectAccessor") => {
            PpcImportDispatcherTarget::ObjectSupportGetAccessor
        }
        ("ObjectSupportLib", "AECallObjectAccessor") => {
            PpcImportDispatcherTarget::ObjectSupportCallAccessor
        }
        ("ObjectSupportLib", "AEDisposeToken") => {
            PpcImportDispatcherTarget::ObjectSupportDisposeToken
        }
        ("ObjectSupportLib", "AERemoveObjectAccessor") => {
            PpcImportDispatcherTarget::ObjectSupportRemoveAccessor
        }
        ("ObjectSupportLib", "AESetObjectCallbacks") => {
            PpcImportDispatcherTarget::ObjectSupportSetCallbacks
        }
        ("ObjectSupportLib", "AEResolve") => PpcImportDispatcherTarget::ObjectSupportResolve,
        ("ObjectSupportLib", "CreateObjSpecifier") => {
            PpcImportDispatcherTarget::ObjectSupportCompatibility
        }
        ("OpenGLLibrary", "aglChoosePixelFormat") => {
            PpcImportDispatcherTarget::AglChoosePixelFormat
        }
        ("OpenGLLibrary", "aglGetError") => PpcImportDispatcherTarget::AglGetError,
        ("OpenGLLibrary", "aglDescribePixelFormat") => {
            PpcImportDispatcherTarget::AglDescribePixelFormat
        }
        ("OpenGLLibrary", "aglDestroyPixelFormat") => {
            PpcImportDispatcherTarget::AglDestroyPixelFormat
        }
        ("OpenGLLibrary", "aglCreateContext") => PpcImportDispatcherTarget::AglCreateContext,
        ("OpenGLLibrary", "aglDestroyContext") => PpcImportDispatcherTarget::AglDestroyContext,
        ("OpenGLLibrary", "aglSetCurrentContext") => {
            PpcImportDispatcherTarget::AglSetCurrentContext
        }
        ("OpenGLLibrary", "aglGetCurrentContext") => {
            PpcImportDispatcherTarget::AglGetCurrentContext
        }
        ("OpenGLLibrary", "aglSetDrawable") => PpcImportDispatcherTarget::AglSetDrawable,
        ("OpenGLLibrary", "aglGetDrawable") => PpcImportDispatcherTarget::AglGetDrawable,
        ("OpenGLLibrary", "aglUpdateContext") => PpcImportDispatcherTarget::AglUpdateContext,
        ("OpenGLLibrary", "aglSwapBuffers") => PpcImportDispatcherTarget::AglSwapBuffers,
        _ => PpcImportDispatcherTarget::Unsupported,
    }
}
