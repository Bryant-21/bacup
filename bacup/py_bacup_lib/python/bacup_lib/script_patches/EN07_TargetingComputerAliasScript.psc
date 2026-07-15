Bool Function IsLocalLaunchPrepComplete()
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    Return personalQuest != None && personalQuest.IsStageDone(530)
EndFunction

Function SayLocalTopic(Topic akTopic)
    Actor launchVoice = NukeLaunchVoice.GetReference() as Actor
    If launchVoice != None && akTopic != None
        launchVoice.Say(akTopic)
    EndIf
EndFunction

Event OnAliasInit()
    ObjectReference targetingComputer = GetReference()
    If targetingComputer != None
        targetingComputer.BlockActivation(True, False)
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || bPermitActivation
        Return
    EndIf
    bPermitActivation = True

    If !IsLocalLaunchPrepComplete()
        SayLocalTopic(EN07_LaunchPrepRequired)
        bPermitActivation = False
        Return
    EndIf
    If player.GetValue(LaunchCardValue) < 1.0
        SayLocalTopic(EN07_LaunchCardRequired)
        bPermitActivation = False
        Return
    EndIf
    If player.GetValue(CodeEnteredIndexValue) < 1.0
        SayLocalTopic(EN07_EnterCode)
        bPermitActivation = False
        Return
    EndIf
    If player.GetValue(PlayerLaunchCooldown) > Utility.GetCurrentGameTime()
        SayLocalTopic(EN07_PlayerInCooldown)
        bPermitActivation = False
        Return
    EndIf

    Quest masterQuest = Game.GetFormFromFile(0x002D0F67, "SeventySix.esm") as Quest
    If masterQuest != None && !masterQuest.IsRunning()
        masterQuest.Start()
    EndIf
    EN07_NukeMasterScript masterScript = masterQuest as EN07_NukeMasterScript
    If masterScript == None || !masterScript.BeginLocalLaunch(iSiloID, iLaunchID, player, BlastTarget)
        SayLocalTopic(EN07_TargetComputerDenialTopic)
        bPermitActivation = False
        Return
    EndIf

    player.SetValue(LaunchCardValue, 0.0)
    player.SetValue(CodeEnteredIndexValue, 0.0)
    player.SetValue(CodeEnteredYearValue, 0.0)
    player.SetValue(EN07_Death_LaunchedNuke, 1.0)
    Float cooldownSeconds = EN07_PlayerLaunchCooldownTime.GetValue()
    If cooldownSeconds <= 0.0
        cooldownSeconds = 180.0
    EndIf
    player.SetValue(PlayerLaunchCooldown, Utility.GetCurrentGameTime() + (cooldownSeconds / 86400.0))

    ObjectReference targetingComputer = GetReference()
    If targetingComputer != None
        targetingComputer.BlockActivation(True, False)
    EndIf
    bPermitActivation = False
EndEvent

Function SetLocalBlastTarget(ObjectReference akTarget)
    BlastTarget = akTarget
EndFunction
