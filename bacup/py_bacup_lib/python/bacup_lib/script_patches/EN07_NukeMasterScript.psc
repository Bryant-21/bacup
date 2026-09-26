Event OnQuestInit()
    Int i = 0
    While i < CodeData.Length
        CodeDatum launchData = CodeData[i]
        If i < 3
            ObjectReference exteriorKeypad = launchData.Keypad.GetReference()
            If exteriorKeypad != None
                launchData.KeypadActive.ForceRefTo(exteriorKeypad)
                exteriorKeypad.BlockActivation(False, False)
            EndIf
        Else
            launchData.bIsInCooldown = False
        EndIf
        CodeData[i] = launchData
        i += 1
    EndWhile
EndEvent

Bool Function PrepareLocalSiloEntry(ActorValue akLaunchCardValue, ObjectReference akEntryRef = None)
    ActorValue bravoLaunchCard = Game.GetFormFromFile(0x003E58A1, "SeventySix.esm") as ActorValue
    ActorValue charlieLaunchCard = Game.GetFormFromFile(0x003E58A2, "SeventySix.esm") as ActorValue
    Int siloID = 0
    If akLaunchCardValue == bravoLaunchCard
        siloID = 1
    ElseIf akLaunchCardValue == charlieLaunchCard
        siloID = 2
    EndIf
    If siloID < 0 || siloID >= CodeData.Length || CodeData[siloID].SiloLocation == None
        Return False
    EndIf

    Quest startupQuest = Game.GetFormFromFile(0x0050FDEE, "SeventySix.esm") as Quest
    MSiloStartupQuestScript startup = startupQuest as MSiloStartupQuestScript
    Return startup != None && startup.StartPreparedSilo(CodeData[siloID].SiloLocation, akEntryRef)
EndFunction

Function HandleLocalLaunchCard(ObjectReference akConsoleRef)
    Int i = 3
    While i < CodeData.Length
        CodeDatum launchData = CodeData[i]
        ObjectReference consoleRef = launchData.CardConsole.GetReference()
        If consoleRef == akConsoleRef
            EN07_ExternalKeypadAliasScript keypadAlias = launchData.KeypadActive as EN07_ExternalKeypadAliasScript
            If keypadAlias != None
                keypadAlias.ResetLocalEntry()
            EndIf
            ObjectReference keypadRef = launchData.Keypad.GetReference()
            If keypadRef != None
                launchData.KeypadActive.ForceRefTo(keypadRef)
                keypadRef.BlockActivation(False, False)
            EndIf
            Return
        EndIf
        i += 1
    EndWhile
EndFunction

Function HandleLocalCodeAccepted(ObjectReference akKeypadRef)
    Int i = 3
    While i < CodeData.Length
        CodeDatum launchData = CodeData[i]
        If launchData.Keypad.GetReference() == akKeypadRef
            ObjectReference targetingComputer = launchData.TargetingComputerAlias.GetReference()
            If targetingComputer != None
                targetingComputer.BlockActivation(False, False)
                targetingComputer.SetActivateTextOverride(None)
            EndIf
            Return
        EndIf
        i += 1
    EndWhile
EndFunction

ObjectReference Function ResolveLocalBlastTarget(Int aiSiloID, ObjectReference akRequestedTarget)
    If akRequestedTarget != None
        Return akRequestedTarget
    EndIf
    If MasterFissureMarker != None
        ObjectReference fissureMarker = MasterFissureMarker.GetReference()
        If fissureMarker != None
            Return fissureMarker
        EndIf
    EndIf
    ObjectReference fissureSitePrimeMarker = Game.GetFormFromFile(0x003A8CCF, "SeventySix.esm") as ObjectReference
    If fissureSitePrimeMarker != None
        Return fissureSitePrimeMarker
    EndIf
    If aiSiloID >= 0 && aiSiloID < CodeData.Length
        Return CodeData[aiSiloID].NukeBlastMarker.GetReference()
    EndIf
    Return None
EndFunction

Int Function GetAvailableLocalLaunchID()
    If CodeData.Length < 6
        Debug.Trace("[B21 Nuke] Launch data is incomplete: entries=" + CodeData.Length)
        Return -2
    EndIf
    Int launchID = 3
    While launchID < 6
        CodeDatum launchData = CodeData[launchID]
        Int siloState = iSiloStateOpen
        If launchData.SiloState != None
            siloState = launchData.SiloState.GetValueInt()
        EndIf
        Debug.Trace("[B21 Nuke] silo=" + (launchID - 3) + " cooldown=" + launchData.bIsInCooldown + " state=" + siloState)
        If !launchData.bIsInCooldown && siloState == iSiloStateOpen
            Return launchID
        EndIf
        launchID += 1
    EndWhile
    Return -1
EndFunction

Bool Function BeginLocalLaunch(Int aiSiloID, Int aiLaunchID, Actor akLaunchingPlayer, ObjectReference akRequestedTarget = None)
    If aiSiloID < 0 || aiSiloID > 2 || aiLaunchID < 3 || aiLaunchID >= CodeData.Length
        Debug.Trace("[B21 Nuke] Launch rejected: invalid silo/launch IDs " + aiSiloID + "/" + aiLaunchID)
        Return False
    EndIf

    CodeDatum launchData = CodeData[aiLaunchID]
    If launchData.bIsInCooldown
        Debug.Trace("[B21 Nuke] Launch rejected: silo " + aiSiloID + " is in cooldown.")
        Return False
    EndIf
    If launchData.SiloState != None && launchData.SiloState.GetValueInt() != iSiloStateOpen
        Debug.Trace("[B21 Nuke] Launch rejected: silo " + aiSiloID + " state=" + launchData.SiloState.GetValueInt())
        Return False
    EndIf

    Quest fleeBlastQuest = Game.GetFormFromFile(0x002D0F69, "SeventySix.esm") as Quest
    EN07_FleeBlastQuestScript fleeBlast = fleeBlastQuest as EN07_FleeBlastQuestScript
    If fleeBlast == None || EN07_FleeBlastQuestStartKeyword == None
        Debug.Trace("[B21 Nuke] Launch rejected: Death from Above script or start keyword is missing.")
        Return False
    EndIf
    If fleeBlastQuest.IsRunning() && !fleeBlastQuest.IsStageDone(100)
        Debug.Trace("[B21 Nuke] Launch rejected: another nuclear strike is active.")
        Return False
    EndIf
    If fleeBlastQuest.IsCompleted() || fleeBlastQuest.IsStageDone(100)
        fleeBlastQuest.Stop()
        fleeBlastQuest.Reset()
    EndIf

    CodeDatum blastData = CodeData[aiSiloID]
    ObjectReference blastMarker = blastData.NukeBlastMarker.GetReference()
    ObjectReference blastTarget = ResolveLocalBlastTarget(aiSiloID, akRequestedTarget)
    If blastMarker == None || blastTarget == None
        Debug.Trace("[B21 Nuke] Launch rejected: blast marker=" + blastMarker + " target=" + blastTarget)
        Return False
    EndIf
    If blastMarker != blastTarget
        blastMarker.MoveTo(blastTarget)
    EndIf
    blastMarker.Enable()

    Bool blastStarted = False
    If fleeBlast != None
        Location blastLocation = blastMarker.GetCurrentLocation()
        If !fleeBlast.PrepareLocalBlast(blastMarker, akLaunchingPlayer, aiSiloID, aiLaunchID, blastData.SmokeEffectSpell, blastData.BlastEffectSpell)
            Debug.Trace("[B21 Nuke] Launch rejected: Death from Above preparation failed.")
            Return False
        EndIf
        Bool eventStarted = EN07_FleeBlastQuestStartKeyword.SendStoryEventAndWait(blastLocation, blastMarker, akLaunchingPlayer, aiSiloID, aiLaunchID)
        Debug.Trace("[B21 Nuke] Story event=" + eventStarted + " starting=" + fleeBlastQuest.IsStarting() + " running=" + fleeBlastQuest.IsRunning() + " stage=" + fleeBlastQuest.GetCurrentStageID())
        ; Startup can drop back from running to starting while the engine finishes
        ; promoting alias references, so retry the handoff until it holds.
        Int startPolls = 0
        While !blastStarted && (eventStarted || fleeBlastQuest.IsStarting() || fleeBlastQuest.IsRunning()) && startPolls < 40
            If fleeBlastQuest.IsRunning()
                blastStarted = fleeBlast.BeginLocalBlast(blastMarker, akLaunchingPlayer, aiSiloID, aiLaunchID, blastData.SmokeEffectSpell, blastData.BlastEffectSpell)
            EndIf
            If !blastStarted
                Utility.Wait(0.25)
                startPolls += 1
            EndIf
        EndWhile
        Debug.Trace("[B21 Nuke] Quest startup settled: polls=" + startPolls + " started=" + blastStarted + " starting=" + fleeBlastQuest.IsStarting() + " running=" + fleeBlastQuest.IsRunning())
    EndIf
    If !blastStarted
        blastMarker.Disable()
        Debug.Trace("[B21 Nuke] Death from Above failed to start; launch aborted.")
        Return False
    EndIf
    Nuke_MasterScript codeMaster = Game.GetFormFromFile(0x003CD064, "SeventySix.esm") as Nuke_MasterScript
    If codeMaster != None
        codeMaster.MarkLocalCodeUsed(aiSiloID)
    EndIf
    launchData.bIsInCooldown = True
    launchData.MostRecentLaunch = Utility.GetCurrentGameTime()
    If launchData.SiloState != None
        launchData.SiloState.SetValue(iSiloStateLaunching as Float)
    EndIf
    CodeData[aiLaunchID] = launchData
    Quest fleeSiloQuest = Game.GetFormFromFile(0x002D0F68, "SeventySix.esm") as Quest
    EN07_FleeSiloScript fleeSilo = fleeSiloQuest as EN07_FleeSiloScript
    If fleeSilo != None
        fleeSilo.BeginLocalLaunch(aiSiloID, aiLaunchID, launchData.SiloLocation)
    EndIf
    Return True
EndFunction

Function DetonateLocalBlastFallback(Int aiSiloID)
    If aiSiloID < 0 || aiSiloID > 2 || aiSiloID >= CodeData.Length
        Return
    EndIf
    CodeDatum blastData = CodeData[aiSiloID]
    ObjectReference blastMarker = blastData.NukeBlastMarker.GetReference()
    If blastMarker == None
        Return
    EndIf
    Explosion nukeExplosion = Game.GetFormFromFile(0x0009A224, "SeventySix.esm") as Explosion
    If nukeExplosion != None
        blastMarker.PlaceAtMe(nukeExplosion)
    EndIf
    Actor player = Game.GetPlayer()
    If blastData.BlastEffectSpell != None
        blastData.BlastEffectSpell.Cast(player, player)
    EndIf
    CompleteLocalLaunch(aiSiloID, aiSiloID + 3)
EndFunction

Function CompleteLocalLaunch(Int aiSiloID, Int aiLaunchID)
    If aiLaunchID < 3 || aiLaunchID >= CodeData.Length
        Return
    EndIf
    CodeDatum launchData = CodeData[aiLaunchID]
    If launchData.SiloState != None
        launchData.SiloState.SetValue(iSiloStateCooldown as Float)
    EndIf
    CodeData[aiLaunchID] = launchData

    Quest fleeSiloQuest = Game.GetFormFromFile(0x002D0F68, "SeventySix.esm") as Quest
    EN07_FleeSiloScript fleeSilo = fleeSiloQuest as EN07_FleeSiloScript
    If fleeSilo != None
        fleeSilo.FinishLocalLaunch()
    EndIf

    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If personalQuest != None && personalQuest.IsRunning() && !personalQuest.IsStageDone(1000)
        personalQuest.SetStage(1000)
    EndIf

    Float cooldownSeconds = EN07_SiloResetCooldown.GetValue()
    If cooldownSeconds <= 0.0
        cooldownSeconds = 900.0
    EndIf
    StartTimer(cooldownSeconds, 7011 + aiSiloID)
EndFunction

Function ResetLocalSilo(Int aiSiloID, Int aiLaunchID)
    If aiSiloID < 0 || aiSiloID > 2 || aiLaunchID != aiSiloID + 3 || aiLaunchID >= CodeData.Length
        Return
    EndIf
    CodeDatum launchData = CodeData[aiLaunchID]
    Nuke_MasterScript codeMaster = Game.GetFormFromFile(0x003CD064, "SeventySix.esm") as Nuke_MasterScript
    If codeMaster != None
        codeMaster.RenewLocalCodeAfterLaunch(aiSiloID)
    EndIf
    launchData.bIsInCooldown = False
    If launchData.SiloState != None
        launchData.SiloState.SetValue(iSiloStateOpen as Float)
    EndIf

    ObjectReference targetingComputer = launchData.TargetingComputerAlias.GetReference()
    If targetingComputer != None
        targetingComputer.BlockActivation(True, False)
    EndIf
    ObjectReference keypadRef = launchData.Keypad.GetReference()
    If keypadRef != None
        keypadRef.BlockActivation(True, False)
    EndIf
    ObjectReference consoleRef = launchData.CardConsole.GetReference()
    EN07_LaunchCardReceptacleScript cardConsole = consoleRef as EN07_LaunchCardReceptacleScript
    If cardConsole != None
        cardConsole.ResetLocalCard()
    ElseIf consoleRef != None
        consoleRef.BlockActivation(False, False)
    EndIf
    launchData.KeypadActive.Clear()
    CodeData[aiLaunchID] = launchData

    Quest fleeSiloQuest = Game.GetFormFromFile(0x002D0F68, "SeventySix.esm") as Quest
    EN07_FleeSiloScript fleeSilo = fleeSiloQuest as EN07_FleeSiloScript
    If fleeSilo != None
        fleeSilo.ResetLocalSilo()
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID >= 7001 && aiTimerID <= 7003
        DetonateLocalBlastFallback(aiTimerID - 7001)
    ElseIf aiTimerID >= 7011 && aiTimerID <= 7013
        Int siloID = aiTimerID - 7011
        ResetLocalSilo(siloID, siloID + 3)
    EndIf
EndEvent
