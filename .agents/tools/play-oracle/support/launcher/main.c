/*
 * Launcher - Dynamic test app launcher for Golden Fixture Generator
 *
 * This small app is installed permanently in Mac OS 8.1 Startup Items.
 * It looks for and launches "FixtureGen" on the Unix extfs volume.
 * This eliminates the need to modify System_68K.dsk for each test.
 */

#include <Files.h>
#include <Processes.h>
#include <Quickdraw.h>

/* Path to the test app on the Unix extfs volume */
#define TEST_APP_PATH "\pUnix:FixtureGen"

int main(void) {
  FSSpec appSpec;
  LaunchParamBlockRec launchParams;
  OSErr err;

  /* Initialize QuickDraw (required for app launch) */
  InitGraf(&qd.thePort);

  /* Find the test app on the Unix volume */
  err = FSMakeFSSpec(0, 0, TEST_APP_PATH, &appSpec);
  if (err != noErr) {
    /* App not found - just exit silently */
    ExitToShell();
    return 1;
  }

  /* Set up launch parameters */
  launchParams.launchBlockID = extendedBlock;
  launchParams.launchEPBLength = extendedBlockLen;
  launchParams.launchFileFlags = 0;
  launchParams.launchControlFlags = launchContinue;
  launchParams.launchAppSpec = &appSpec;
  launchParams.launchAppParameters = NULL;

  /* Launch the test app */
  err = LaunchApplication(&launchParams);

  /* If launch failed, exit */
  ExitToShell();

  return 0;
}
