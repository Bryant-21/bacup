Event OnQuestInit()
    Int i = 3
    While i < CodeData.Length
        CodeDatum launchData = CodeData[i]
        launchData.bIsInCooldown = False
        CodeData[i] = launchData
        i += 1
    EndWhile
EndEvent

Function HandleLocalLaunchCard(ObjectReference akConsoleRef)
    Int i = 3
    While i < CodeData.Length
        CodeDatum launchData = CodeData[i]
        ObjectReference consoleRef = launchData.CardConsole.GetReference()
        If consoleRef == akConsoleRef
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

Bool Function BeginLocalLaunch(Int aiSiloID, Int aiLaunchID, Actor akLaunchingPlayer, ObjectReference akRequestedTarget = None)
    If aiSiloID < 0 || aiSiloID > 2 || aiLaunchID < 3 || aiLaunchID >= CodeData.Length
        Return False
    EndIf

    CodeDatum launchData = CodeData[aiLaunchID]
    If launchData.bIsInCooldown
        Return False
    EndIf
    If launchData.SiloState != None && launchData.SiloState.GetValueInt() != iSiloStateOpen
        Return False
    EndIf

    CodeDatum blastData = CodeData[aiSiloID]
    ObjectReference blastMarker = blastData.NukeBlastMarker.GetReference()
    ObjectReference blastTarget = ResolveLocalBlastTarget(aiSiloID, akRequestedTarget)
    If blastMarker == None || blastTarget == None
        Return False
    EndIf
    If blastMarker != blastTarget
        blastMarker.MoveTo(blastTarget)
    EndIf
    blastMarker.Enable()

    launchData.bIsInCooldown = True
    launchData.MostRecentLaunch = Utility.GetCurrentGameTime()
    If launchData.SiloState != None
        launchData.SiloState.SetValue(iSiloStateLaunching as Float)
    EndIf
    CodeData[aiLaunchID] = launchData
    iDebugNukeRegionIndex = aiSiloID

    Quest fleeSiloQuest = Game.GetFormFromFile(0x002D0F68, "SeventySix.esm") as Quest
    EN07_FleeSiloScript fleeSilo = fleeSiloQuest as EN07_FleeSiloScript
    If fleeSilo != None
        fleeSilo.BeginLocalLaunch(aiSiloID, aiLaunchID, launchData.SiloLocation)
    EndIf

    Quest fleeBlastQuest = Game.GetFormFromFile(0x002D0F69, "SeventySix.esm") as Quest
    EN07_FleeBlastQuestScript fleeBlast = fleeBlastQuest as EN07_FleeBlastQuestScript
    Bool blastStarted = False
    If fleeBlast != None
        blastStarted = fleeBlast.BeginLocalBlast(blastMarker, akLaunchingPlayer, aiSiloID, aiLaunchID, blastData.SmokeEffectSpell, blastData.BlastEffectSpell)
    EndIf
    If !blastStarted
        StartTimer(180.0, 7001)
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

    Float cooldownSeconds = EN07_SiloResetCooldown.GetValue()
    If cooldownSeconds <= 0.0
        cooldownSeconds = 900.0
    EndIf
    iDebugNukeRegionIndex = aiSiloID
    StartTimer(cooldownSeconds, 7002)
EndFunction

Function ResetLocalSilo(Int aiSiloID, Int aiLaunchID)
    If aiLaunchID < 3 || aiLaunchID >= CodeData.Length
        Return
    EndIf
    CodeDatum launchData = CodeData[aiLaunchID]
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
    If aiTimerID == 7001
        DetonateLocalBlastFallback(iDebugNukeRegionIndex)
    ElseIf aiTimerID == 7002
        ResetLocalSilo(iDebugNukeRegionIndex, iDebugNukeRegionIndex + 3)
    EndIf
EndEvent
