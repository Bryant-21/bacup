Bool Function IsLocalLaunchPrepComplete()
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    Return personalQuest != None && personalQuest.IsStageDone(530)
EndFunction

Function SayLocalTopic(Topic akTopic)
    Actor launchVoice = NuclearLaunchVoice.GetReference() as Actor
    If launchVoice != None && akTopic != None
        launchVoice.Say(akTopic)
    EndIf
EndFunction

Event OnAliasInit()
    bPermitActivation = True
    ObjectReference keypadRef = GetReference()
    If keypadRef != None
        keypadRef.BlockActivation(False, False)
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || !bPermitActivation
        Return
    EndIf
    If !IsLocalLaunchPrepComplete()
        SayLocalTopic(EN07_LaunchPrepRequired)
        Return
    EndIf
    If player.GetValue(LaunchCardValue) < 1.0
        SayLocalTopic(EN07_LaunchCardRequired)
        Return
    EndIf
    If player.GetValue(PlayerLaunchCooldown) > Utility.GetCurrentGameTime()
        SayLocalTopic(EN07_PlayerInCooldown)
        Return
    EndIf

    bPermitActivation = False
    CurrentCode = 1
    player.SetValue(CodeEnteredIndexValue, 1.0)
    player.SetValue(CodeEnteredYearValue, Utility.GetCurrentGameTime())
    EN07_EnteredCorrectCode.Show()
    SayLocalTopic(EN07_AccessGranted)

    ObjectReference keypadRef = GetReference()
    If keypadRef != None
        keypadRef.BlockActivation(True, False)
    EndIf
    If LinkedAccessPanel != None
        ObjectReference accessPanel = LinkedAccessPanel.GetReference()
        If accessPanel != None
            accessPanel.BlockActivation(False, False)
        EndIf
    EndIf

    Quest masterQuest = Game.GetFormFromFile(0x002D0F67, "SeventySix.esm") as Quest
    EN07_NukeMasterScript masterScript = masterQuest as EN07_NukeMasterScript
    If masterScript != None
        masterScript.HandleLocalCodeAccepted(keypadRef)
    EndIf
EndEvent
