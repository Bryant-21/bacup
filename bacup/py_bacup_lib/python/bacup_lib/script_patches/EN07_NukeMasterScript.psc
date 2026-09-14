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
    Quest fleeSiloQuest = Game.GetFormFromFile(0x002D0F68, "SeventySix.esm") as Quest
    EN07_FleeSiloScript fleeSilo = fleeSiloQuest as EN07_FleeSiloScript
    If fleeSilo != None
        fleeSilo.BeginLocalLaunch(aiSiloID, aiLaunchID, launchData.SiloLocation)
    EndIf

    Quest fleeBlastQuest = Game.GetFormFromFile(0x002D0F69, "SeventySix.esm") as Quest
    EN07_FleeBlastQuestScript fleeBlast = fleeBlastQuest as EN07_FleeBlastQuestScript
    Bool blastStarted = False
    If fleeBlast != None
        ReferenceAlias blastAlias = fleeBlastQuest.GetAlias(0) as ReferenceAlias
        ReferenceAlias launchingPlayerAlias = fleeBlastQuest.GetAlias(14) as ReferenceAlias
        LocationAlias triggerLocationAlias = fleeBlastQuest.GetAlias(8) as LocationAlias
        If blastAlias != None
            blastAlias.ForceRefTo(blastMarker)
        EndIf
        If launchingPlayerAlias != None
            launchingPlayerAlias.ForceRefTo(akLaunchingPlayer)
        EndIf
        Location blastLocation = blastMarker.GetCurrentLocation()
        If triggerLocationAlias != None && blastLocation != None
            triggerLocationAlias.ForceLocationTo(blastLocation)
        EndIf
        EN07_FleeBlastQuestStartKeyword.SendStoryEventAndWait(blastLocation, blastMarker, akLaunchingPlayer, aiSiloID, aiLaunchID)
        blastStarted = fleeBlast.BeginLocalBlast(blastMarker, akLaunchingPlayer, aiSiloID, aiLaunchID, blastData.SmokeEffectSpell, blastData.BlastEffectSpell)
    EndIf
    If !blastStarted
        StartTimer(180.0, 7001 + aiSiloID)
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
    If aiTimerID >= 7001 && aiTimerID <= 7003
        DetonateLocalBlastFallback(aiTimerID - 7001)
    ElseIf aiTimerID >= 7011 && aiTimerID <= 7013
        Int siloID = aiTimerID - 7011
        ResetLocalSilo(siloID, siloID + 3)
    EndIf
EndEvent
