/*
 * PlayLauncher - Game launcher with tick reporter for BasiliskII play scripts.
 *
 * Placed on extfs as "FixtureGen" (what the System_68K Startup Items launcher
 * looks for). This app:
 * 1. Launches the real game app ("_PlayTarget" on the Unix volume)
 * 2. Writes the initial game-start Ticks value to "_tick_baseline"
 * 3. Writes the current Ticks value to "_ticks" every ~0.5 seconds
 *
 * The host-side play script runner reads "_ticks" from the shared extfs
 * directory to synchronize with the Mac's actual 60Hz tick counter.
 */

#include <Files.h>
#include <Folders.h>
#include <Resources.h>
#include <Processes.h>
#include <Quickdraw.h>
#include <Events.h>

QDGlobals qd;

#define GAME_APP_PATH "\pUnix:_PlayTarget"
#define GAME_VOLUME_NAME "\pUnix"
#define TICKS_FILE_PATH "\pUnix:_ticks"
#define TICK_BASELINE_FILE_PATH "\pUnix:_tick_baseline"
#define LAUNCH_STATUS_FILE_PATH "\pUnix:_launch_status"
#define LAUNCH_READY_FILE_PATH "\pUnix:_launch_ready"
#define PLAY_TIME_FILE_PATH "\pUnix:_play_time"
#define PRELAUNCH_CREATE_DIRS_PATH "\pUnix:_prelaunch_create_dirs"
#define PRELAUNCH_DELETE_PATHS_PATH "\pUnix:_prelaunch_delete_paths"
#define PRELAUNCH_BOOT_COPY_MANIFEST_PATH "\pUnix:_prelaunch_boot_copy_manifest"
#define BOOT_STAGE_DIR_PATH "\p:Systemless Play"
#define BOOT_STAGE_APP_PATH "\p:Systemless Play:_PlayTarget"
#define BOOT_STAGE_PATH_PREFIX ":Systemless Play"
#define UNIX_PATH_PREFIX "Unix"
#define PRELAUNCH_LINE_MAX 255
#define COPY_BUFFER_SIZE 16384
#define EV_PREFS_PATH "\pMacintoshHD:System Folder:Preferences:Escape Velocity Prefs"
#define EV_LAST_PILOT_PATH "\pMacintoshHD:System Folder:Preferences:Last Pilot"

/* Script Manager constant - smSystemScript = 0 (Roman) */
#ifndef smSystemScript
#define smSystemScript 0
#endif

/* Ticks low-memory global at 0x016A */
#define LMGetTicks() (*(unsigned long *)0x016A)
#define LMSetTime(x) (*(unsigned long *)0x020C = (x))

pascal OSErr SetDateTime(unsigned long time) = { 0xA03A };

static char copyBuffer[COPY_BUFFER_SIZE];

static void ClearBytes(void *ptr, long len);

static void WriteTickValue(ConstStr255Param path, unsigned long ticks)
{
    short refNum;
    long count;
    OSErr err;
    FSSpec spec;

    err = FSOpen(path, 0, &refNum);
    if (err == fnfErr) {
        err = Create(path, 0, 'PLAY', 'DATA');
        if (err != noErr) return;
        err = FSOpen(path, 0, &refNum);
    }
    if (err != noErr) return;

    SetFPos(refNum, fsFromStart, 0);
    count = 4;
    FSWrite(refNum, &count, &ticks);
    FSClose(refNum);

    if (FSMakeFSSpec(0, 0, path, &spec) == noErr) {
        FlushVol(NULL, spec.vRefNum);
    }
}

static void WriteTicks(void)
{
    WriteTickValue(TICKS_FILE_PATH, LMGetTicks());
}

static int ReadUint32File(ConstStr255Param path, unsigned long *value)
{
    short refNum;
    long count;
    OSErr err;

    err = FSOpen(path, 0, &refNum);
    if (err != noErr) return 0;

    count = 4;
    err = FSRead(refNum, &count, value);
    FSClose(refNum);
    return err == noErr && count == 4;
}

static void DeletePreferencesFile(ConstStr255Param path)
{
    FSDelete(path, 0);
}

static short LiteralLength(const char *literal)
{
    short len;

    len = 0;
    while (literal[len] != 0) len++;
    return len;
}

static short PascalStringEqualsLiteral(ConstStr255Param value,
                                        const char *literal)
{
    short literalLen;
    short i;

    literalLen = LiteralLength(literal);
    if (value[0] != literalLen) return 0;
    for (i = 0; i < literalLen; i++) {
        if (value[i + 1] != literal[i]) return 0;
    }
    return 1;
}

static OSErr FindMountedInstallerApplication(FSSpec *installerSpec)
{
    HParamBlockRec volumePB;
    CInfoPBRec catalogPB;
    Str255 volumeName;
    Str255 itemName;
    short systemVRefNum;
    long systemDirID;
    short volumeIndex;
    short itemIndex;
    OSErr err;

    err = FindFolder(kOnSystemDisk, kSystemFolderType, kDontCreateFolder,
                     &systemVRefNum, &systemDirID);
    if (err != noErr) return err;

    for (volumeIndex = 1; ; volumeIndex++) {
        ClearBytes(&volumePB, sizeof(volumePB));
        volumePB.volumeParam.ioNamePtr = volumeName;
        volumePB.volumeParam.ioVRefNum = 0;
        volumePB.volumeParam.ioVolIndex = volumeIndex;
        err = PBHGetVInfoSync(&volumePB);
        if (err != noErr) break;

        if (volumePB.volumeParam.ioVRefNum == systemVRefNum ||
            PascalStringEqualsLiteral(volumeName, "Unix")) {
            continue;
        }

        for (itemIndex = 1; ; itemIndex++) {
            ClearBytes(&catalogPB, sizeof(catalogPB));
            catalogPB.hFileInfo.ioNamePtr = itemName;
            catalogPB.hFileInfo.ioVRefNum = volumePB.volumeParam.ioVRefNum;
            catalogPB.hFileInfo.ioFDirIndex = itemIndex;
            catalogPB.hFileInfo.ioDirID = fsRtDirID;
            err = PBGetCatInfoSync(&catalogPB);
            if (err != noErr) break;

            if ((catalogPB.hFileInfo.ioFlAttrib & ioDirMask) == 0 &&
                catalogPB.hFileInfo.ioFlFndrInfo.fdType == 'APPL') {
                return FSMakeFSSpec(volumePB.volumeParam.ioVRefNum,
                                    fsRtDirID, itemName, installerSpec);
            }
        }
    }

    return fnfErr;
}

static OSErr LaunchSpec(FSSpec *appSpec, short dontSwitch)
{
    LaunchParamBlockRec launchParams;
    char *launchParamBytes;
    long i;

    launchParamBytes = (char *)&launchParams;
    for (i = 0; i < sizeof(launchParams); i++) {
        launchParamBytes[i] = 0;
    }
    launchParams.launchBlockID = extendedBlock;
    launchParams.launchEPBLength = extendedBlockLen;
    launchParams.launchFileFlags = 0;
    launchParams.launchControlFlags = launchContinue | launchNoFileFlags;
    if (dontSwitch) {
        launchParams.launchControlFlags |= launchDontSwitch;
    }
    launchParams.launchAppSpec = appSpec;
    launchParams.launchAppParameters = NULL;
    return LaunchApplication(&launchParams);
}

static short LineEqualsLiteral(char *line, short len, const char *literal)
{
    short literalLen;
    short i;

    literalLen = LiteralLength(literal);
    if (len != literalLen) return 0;
    for (i = 0; i < len; i++) {
        if (line[i] != literal[i]) return 0;
    }
    return 1;
}

static short LineHasPrefix(char *line, short len, const char *prefix)
{
    short prefixLen;
    short i;

    prefixLen = LiteralLength(prefix);
    if (len < prefixLen) return 0;
    for (i = 0; i < prefixLen; i++) {
        if (line[i] != prefix[i]) return 0;
    }
    return 1;
}

static void CreatePrelaunchDirectory(ConstStr255Param path)
{
    FSSpec spec;
    long createdDirID;
    OSErr err;

    err = FSMakeFSSpec(0, 0, path, &spec);
    if (err != noErr && err != fnfErr) return;

    err = DirCreate(spec.vRefNum, spec.parID, spec.name, &createdDirID);
    if (err == noErr || err == dupFNErr) return;
}

static void CreateChildDirectoryComponents(short vRefNum, long parentDirID,
                                           char *path, short len)
{
    Str255 name;
    short start;
    short end;
    short nameLen;
    short i;
    long createdDirID;
    OSErr err;

    start = 0;
    while (start < len) {
        while (start < len && path[start] == ':') start++;
        if (start >= len) return;

        end = start;
        while (end < len && path[end] != ':') end++;
        nameLen = end - start;
        if (nameLen <= 0 || nameLen > 255) return;

        name[0] = (unsigned char)nameLen;
        for (i = 0; i < nameLen; i++) {
            name[i + 1] = path[start + i];
        }

        err = DirCreate(vRefNum, parentDirID, name, &createdDirID);
        if (err == noErr) {
            parentDirID = createdDirID;
        } else if (err != dupFNErr) {
            return;
        }

        start = end + 1;
    }
}

static short CreateSystemFolderRelativeDirectory(char *path, short pathLen)
{
    short vRefNum;
    long dirID;
    short prefixLen;
    OSErr err;

    if (LineEqualsLiteral(path, pathLen, "System Folder")) {
        err = FindFolder(kOnSystemDisk, kSystemFolderType, kCreateFolder,
                         &vRefNum, &dirID);
        return err == noErr;
    }

    if (!LineHasPrefix(path, pathLen, "System Folder:Preferences")) {
        return 0;
    }

    prefixLen = LiteralLength("System Folder:Preferences");
    if (pathLen > prefixLen && path[prefixLen] != ':') {
        return 0;
    }

    err = FindFolder(kOnSystemDisk, kPreferencesFolderType, kCreateFolder,
                     &vRefNum, &dirID);
    if (err != noErr) return 1;
    if (pathLen == prefixLen) return 1;

    CreateChildDirectoryComponents(vRefNum, dirID,
                                   path + prefixLen + 1,
                                   pathLen - prefixLen - 1);
    return 1;
}

static void ProcessPrelaunchCreateDirLine(char *line, short len)
{
    Str255 path;
    short start;
    short end;
    short pathLen;
    short i;

    start = 0;
    end = len;
    while (start < end && (line[start] == ' ' || line[start] == '\t')) start++;
    while (end > start &&
           (line[end - 1] == ' ' || line[end - 1] == '\t')) {
        end--;
    }

    pathLen = end - start;
    if (pathLen <= 0 || pathLen > 255) return;

    if (CreateSystemFolderRelativeDirectory(line + start, pathLen)) return;

    path[0] = (unsigned char)pathLen;
    for (i = 0; i < pathLen; i++) {
        path[i + 1] = line[start + i];
    }

    CreatePrelaunchDirectory(path);
}

static void ApplyPrelaunchCreateDirs(void)
{
    short refNum;
    OSErr err;
    char buffer[128];
    char line[PRELAUNCH_LINE_MAX];
    short lineLen;
    short lineOverflow;
    long count;
    long i;
    char ch;

    err = FSOpen(PRELAUNCH_CREATE_DIRS_PATH, 0, &refNum);
    if (err != noErr) return;

    lineLen = 0;
    lineOverflow = 0;
    for (;;) {
        count = sizeof(buffer);
        err = FSRead(refNum, &count, buffer);
        if (err != noErr && err != eofErr) break;

        for (i = 0; i < count; i++) {
            ch = buffer[i];
            if (ch == '\r' || ch == '\n') {
                if (!lineOverflow) {
                    ProcessPrelaunchCreateDirLine(line, lineLen);
                }
                lineLen = 0;
                lineOverflow = 0;
            } else if (lineLen < PRELAUNCH_LINE_MAX) {
                line[lineLen++] = ch;
            } else {
                lineOverflow = 1;
            }
        }

        if (err == eofErr || count == 0) break;
    }

    if (lineLen > 0 && !lineOverflow) {
        ProcessPrelaunchCreateDirLine(line, lineLen);
    }

    FSClose(refNum);
}

static void ProcessPrelaunchDeletePathLine(char *line, short len)
{
    Str255 path;
    short start;
    short end;
    short pathLen;
    short i;

    start = 0;
    end = len;
    while (start < end && (line[start] == ' ' || line[start] == '\t')) start++;
    while (end > start &&
           (line[end - 1] == ' ' || line[end - 1] == '\t')) {
        end--;
    }

    pathLen = end - start;
    if (pathLen <= 0 || pathLen > 255) return;

    path[0] = (unsigned char)pathLen;
    for (i = 0; i < pathLen; i++) {
        path[i + 1] = line[start + i];
    }

    FSDelete(path, 0);
}

static void ApplyPrelaunchDeletePaths(void)
{
    short refNum;
    OSErr err;
    char buffer[128];
    char line[PRELAUNCH_LINE_MAX];
    short lineLen;
    short lineOverflow;
    long count;
    long i;
    char ch;

    err = FSOpen(PRELAUNCH_DELETE_PATHS_PATH, 0, &refNum);
    if (err != noErr) return;

    lineLen = 0;
    lineOverflow = 0;
    for (;;) {
        count = sizeof(buffer);
        err = FSRead(refNum, &count, buffer);
        if (err != noErr && err != eofErr) break;

        for (i = 0; i < count; i++) {
            ch = buffer[i];
            if (ch == '\r' || ch == '\n') {
                if (!lineOverflow) {
                    ProcessPrelaunchDeletePathLine(line, lineLen);
                }
                lineLen = 0;
                lineOverflow = 0;
            } else if (lineLen < PRELAUNCH_LINE_MAX) {
                line[lineLen++] = ch;
            } else {
                lineOverflow = 1;
            }
        }

        if (err == eofErr || count == 0) break;
    }

    if (lineLen > 0 && !lineOverflow) {
        ProcessPrelaunchDeletePathLine(line, lineLen);
    }

    FSClose(refNum);
}

static short BuildManifestPath(Str255 path, const char *prefix,
                               char *rel, short relLen)
{
    short prefixLen;
    short totalLen;
    short i;
    short pos;

    prefixLen = LiteralLength(prefix);
    totalLen = prefixLen;
    if (relLen > 0) totalLen += 1 + relLen;
    if (totalLen > 255) return 0;

    path[0] = (unsigned char)totalLen;
    pos = 1;
    for (i = 0; i < prefixLen; i++) path[pos++] = prefix[i];
    if (relLen > 0) {
        path[pos++] = ':';
        for (i = 0; i < relLen; i++) path[pos++] = rel[i];
    }
    return 1;
}

static void ClearBytes(void *ptr, long len)
{
    char *bytes;
    long i;

    bytes = (char *)ptr;
    for (i = 0; i < len; i++) bytes[i] = 0;
}

static short SetDefaultDirectory(ConstStr255Param path)
{
    FSSpec spec;
    CInfoPBRec info;
    OSErr err;

    err = FSMakeFSSpec(0, 0, path, &spec);
    if (err != noErr) return 0;

    ClearBytes(&info, sizeof(info));
    info.dirInfo.ioNamePtr = spec.name;
    info.dirInfo.ioVRefNum = spec.vRefNum;
    info.dirInfo.ioFDirIndex = 0;
    info.dirInfo.ioDrDirID = spec.parID;
    err = PBGetCatInfoSync(&info);
    if (err != noErr) return 0;

    err = HSetVol(NULL, spec.vRefNum, info.dirInfo.ioDrDirID);
    return err == noErr;
}

static short CopyOpenFork(short srcRefNum, short dstRefNum)
{
    OSErr err;
    OSErr writeErr;
    long count;
    long writeCount;

    SetFPos(srcRefNum, fsFromStart, 0);
    SetFPos(dstRefNum, fsFromStart, 0);
    SetEOF(dstRefNum, 0);

    for (;;) {
        count = COPY_BUFFER_SIZE;
        err = FSRead(srcRefNum, &count, copyBuffer);
        if (count > 0) {
            writeCount = count;
            writeErr = FSWrite(dstRefNum, &writeCount, copyBuffer);
            if (writeErr != noErr || writeCount != count) return 0;
            WriteTicks();
        }
        if (err == eofErr || count == 0) return 1;
        if (err != noErr) return 0;
    }
}

static short CopyDataFork(FSSpec *srcSpec, FSSpec *dstSpec)
{
    short srcRefNum;
    short dstRefNum;
    OSErr err;
    short ok;

    err = FSpOpenDF(srcSpec, fsRdPerm, &srcRefNum);
    if (err != noErr) return 0;

    err = FSpOpenDF(dstSpec, fsWrPerm, &dstRefNum);
    if (err != noErr) {
        FSClose(srcRefNum);
        return 0;
    }

    ok = CopyOpenFork(srcRefNum, dstRefNum);
    FSClose(dstRefNum);
    FSClose(srcRefNum);
    return ok;
}

static short CopyResourceFork(FSSpec *srcSpec, FSSpec *dstSpec,
                              OSType creator, OSType fileType)
{
    short srcRefNum;
    short dstRefNum;
    OSErr err;
    short ok;

    err = FSpOpenRF(srcSpec, fsRdPerm, &srcRefNum);
    if (err != noErr) return 1;

    FSpCreateResFile(dstSpec, creator, fileType, smSystemScript);
    err = FSpOpenRF(dstSpec, fsWrPerm, &dstRefNum);
    if (err != noErr) {
        FSClose(srcRefNum);
        return 0;
    }

    ok = CopyOpenFork(srcRefNum, dstRefNum);
    FSClose(dstRefNum);
    FSClose(srcRefNum);
    return ok;
}

static short CopyBootStagedFile(char *rel, short relLen)
{
    Str255 srcPath;
    Str255 dstPath;
    FSSpec srcSpec;
    FSSpec dstSpec;
    FInfo info;
    OSType creator;
    OSType fileType;
    OSErr err;

    if (!BuildManifestPath(srcPath, UNIX_PATH_PREFIX, rel, relLen)) return 0;
    if (!BuildManifestPath(dstPath, BOOT_STAGE_PATH_PREFIX, rel, relLen)) return 0;

    err = FSMakeFSSpec(0, 0, srcPath, &srcSpec);
    if (err != noErr) return 0;
    err = FSMakeFSSpec(0, 0, dstPath, &dstSpec);
    if (err != noErr && err != fnfErr) return 0;

    creator = '????';
    fileType = '????';
    if (FSpGetFInfo(&srcSpec, &info) == noErr) {
        creator = info.fdCreator;
        fileType = info.fdType;
    }

    FSDelete(dstPath, 0);
    err = FSpCreate(&dstSpec, creator, fileType, smSystemScript);
    if (err != noErr && err != dupFNErr) return 0;

    if (!CopyDataFork(&srcSpec, &dstSpec)) return 0;
    if (!CopyResourceFork(&srcSpec, &dstSpec, creator, fileType)) return 0;
    return 1;
}

static short CreateBootStageDirectory(char *rel, short relLen)
{
    Str255 dstPath;

    if (!BuildManifestPath(dstPath, BOOT_STAGE_PATH_PREFIX, rel, relLen)) return 0;
    CreatePrelaunchDirectory(dstPath);
    return 1;
}

static short ProcessBootCopyManifestLine(char *line, short len)
{
    char kind;
    char *rel;
    short relLen;

    if (len < 3) return 0;
    kind = line[0];
    if (line[1] != '\t') return 0;
    rel = line + 2;
    relLen = len - 2;

    if (kind == 'D') return CreateBootStageDirectory(rel, relLen);
    if (kind == 'F') return CopyBootStagedFile(rel, relLen);
    return 0;
}

static short StageGameToBootDisk(void)
{
    short systemVRefNum;
    long systemDirID;
    short refNum;
    OSErr err;
    char buffer[128];
    char line[PRELAUNCH_LINE_MAX];
    short lineLen;
    short lineOverflow;
    short ok;
    FSSpec copiedSpec;
    long count;
    long i;
    char ch;

    err = FSOpen(PRELAUNCH_BOOT_COPY_MANIFEST_PATH, 0, &refNum);
    if (err != noErr) return 0;

    /* Resolve the startup volume rather than assuming the BasiliskII disk's
       historical "MacOS7" label. SheepShaver fixtures commonly boot Mac OS 8
       or 9 volumes with different names. */
    err = FindFolder(kOnSystemDisk, kSystemFolderType, kDontCreateFolder,
                     &systemVRefNum, &systemDirID);
    if (err != noErr || HSetVol(NULL, systemVRefNum, fsRtDirID) != noErr) {
        FSClose(refNum);
        return 0;
    }

    CreatePrelaunchDirectory(BOOT_STAGE_DIR_PATH);
    ok = 1;
    lineLen = 0;
    lineOverflow = 0;
    for (;;) {
        count = sizeof(buffer);
        err = FSRead(refNum, &count, buffer);
        if (err != noErr && err != eofErr) {
            ok = 0;
            break;
        }

        for (i = 0; i < count; i++) {
            ch = buffer[i];
            if (ch == '\r' || ch == '\n') {
                if (!lineOverflow && lineLen > 0) {
                    if (!ProcessBootCopyManifestLine(line, lineLen)) ok = 0;
                }
                lineLen = 0;
                lineOverflow = 0;
            } else if (lineLen < PRELAUNCH_LINE_MAX) {
                line[lineLen++] = ch;
            } else {
                lineOverflow = 1;
            }
        }

        if (err == eofErr || count == 0) break;
    }

    if (lineLen > 0 && !lineOverflow) {
        if (!ProcessBootCopyManifestLine(line, lineLen)) ok = 0;
    }

    FSClose(refNum);
    if (!ok) return 0;
    err = FSMakeFSSpec(0, 0, BOOT_STAGE_APP_PATH, &copiedSpec);
    return err == noErr;
}

int main(void)
{
    FSSpec appSpec;
    FSSpec mountedInstallerSpec;
    FInfo appInfo;
    OSErr err;
    unsigned long lastWrite;
    unsigned long playTime;
    unsigned long followupAt;
    short launchFromBootDisk;
    short launchMountedInstaller;
    short attemptedMountedInstaller;

    InitGraf(&qd.thePort);

    if (ReadUint32File(PLAY_TIME_FILE_PATH, &playTime)) {
        SetDateTime(playTime);
        LMSetTime(playTime);
    }

    DeletePreferencesFile(EV_PREFS_PATH);
    DeletePreferencesFile(EV_LAST_PILOT_PATH);
    ApplyPrelaunchDeletePaths();
    ApplyPrelaunchCreateDirs();

    /* Write the game-start baseline before launch. The host-side runner uses
       this to normalize all later `_ticks` samples so both emulators see the
       same "game start = tick 0" timeline. */
    WriteTickValue(TICK_BASELINE_FILE_PATH, LMGetTicks());
    WriteTicks();

    launchFromBootDisk = StageGameToBootDisk();

    /* Find the game app */
    err = FSMakeFSSpec(0, 0,
                       launchFromBootDisk ? BOOT_STAGE_APP_PATH : GAME_APP_PATH,
                       &appSpec);
    if (err != noErr) {
        WriteTickValue(LAUNCH_STATUS_FILE_PATH, (unsigned long)(long)err);
        ExitToShell();
        return 1;
    }
    launchMountedInstaller = 0;
    if (FSpGetFInfo(&appSpec, &appInfo) == noErr &&
        appInfo.fdType == 'APPL' && appInfo.fdCreator == 'oneb') {
        launchMountedInstaller = 1;
    }

    /* Match Finder-style launches from the game folder: older games often
       open sibling data with relative paths and vRefNum 0. The staged game
       files live at the Unix extfs volume root, so make that the default
       volume before LaunchApplication. */
    if (launchFromBootDisk) {
        SetDefaultDirectory(BOOT_STAGE_DIR_PATH);
    } else {
        SetVol(GAME_VOLUME_NAME, 0);
    }

    /* Launch the game — do NOT use launchContinue so the game becomes
       the frontmost application and we exit. Instead, we start a tick
       writing loop BEFORE launch and rely on the game taking over. */

    /* Start writing ticks in a loop. We use launchContinue so we can
       keep writing ticks after the game launches, but we must NOT
       call SystemTask or do anything that keeps us in the foreground.
       The game will take over the screen. */
    WriteTickValue(LAUNCH_READY_FILE_PATH, 0);
    err = LaunchSpec(&appSpec, launchMountedInstaller);
    WriteTickValue(LAUNCH_STATUS_FILE_PATH, (unsigned long)(long)err);
    if (err != noErr) {
        ExitToShell();
        return 1;
    }

    /* Write ticks periodically. Use WaitNextEvent to yield CPU to game. */
    lastWrite = LMGetTicks();
    followupAt = lastWrite + 300;
    attemptedMountedInstaller = 0;
    for (;;) {
        EventRecord event;
        unsigned long now;

        /* Yield to other apps — this lets the game run and draw */
        WaitNextEvent(0, &event, 5, NULL);

        now = LMGetTicks();
        if (launchMountedInstaller && !attemptedMountedInstaller &&
            now >= followupAt) {
            attemptedMountedInstaller = 1;
            err = FindMountedInstallerApplication(&mountedInstallerSpec);
            if (err == noErr) {
                err = LaunchSpec(&mountedInstallerSpec, 0);
            }
        }
        if (now - lastWrite >= 15) {
            WriteTicks();
            lastWrite = now;
        }
    }

    return 0;
}
