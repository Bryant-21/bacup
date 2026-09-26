Bool Function IsLocalLaunchPrepComplete()
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    Return personalQuest != None && personalQuest.IsStageDone(530)
EndFunction

Bool Function HasLocalCodePiece(Actor akPlayer)
    Int firstFormID = 0x003DA648
    ActorValue bravoLaunchCard = Game.GetFormFromFile(0x003E58A1, "SeventySix.esm") as ActorValue
    ActorValue charlieLaunchCard = Game.GetFormFromFile(0x003E58A2, "SeventySix.esm") as ActorValue
    If LaunchCardValue == bravoLaunchCard
        firstFormID = 0x004DE233
    ElseIf LaunchCardValue == charlieLaunchCard
        firstFormID = 0x004DE23B
    EndIf
    Int i = 0
    While i < 8
        Form codePage = Game.GetFormFromFile(firstFormID + i, "SeventySix.esm")
        If codePage != None && akPlayer.GetItemCount(codePage) > 0
            Return True
        EndIf
        i += 1
    EndWhile
    Return False
EndFunction

Bool Function HasLocalSiloAccess(Actor akPlayer)
    If EN05_MQ_Officer != None && (EN05_MQ_Officer.IsCompleted() || EN05_MQ_Officer.IsStageDone(110))
        Return True
    EndIf
    ActorValue completedValue = Game.GetFormFromFile(0x00182162, "SeventySix.esm") as ActorValue
    Return akPlayer != None && completedValue != None && akPlayer.GetValue(completedValue) >= 1.0
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

Function ResetLocalEntry()
    bPermitActivation = True
    CurrentCode = 0
    Actor player = Game.GetPlayer()
    If CodeEnteredIndexValue != None
        player.SetValue(CodeEnteredIndexValue, 0.0)
    EndIf
    If CodeEnteredYearValue != None
        player.SetValue(CodeEnteredYearValue, 0.0)
    EndIf
    ObjectReference keypadRef = GetReference()
    If keypadRef != None
        keypadRef.BlockActivation(False, False)
    EndIf
EndFunction

Int Function LocalSiloID()
    ActorValue bravoLaunchCard = Game.GetFormFromFile(0x003E58A1, "SeventySix.esm") as ActorValue
    ActorValue charlieLaunchCard = Game.GetFormFromFile(0x003E58A2, "SeventySix.esm") as ActorValue
    If LaunchCardValue == bravoLaunchCard
        Return 1
    ElseIf LaunchCardValue == charlieLaunchCard
        Return 2
    EndIf
    Return 0
EndFunction

Function RejectLocalCode()
    Actor player = Game.GetPlayer()
    player.SetValue(LaunchCardValue, 0.0)
    Quest masterQuest = Game.GetFormFromFile(0x002D0F67, "SeventySix.esm") as Quest
    EN07_NukeMasterScript master = masterQuest as EN07_NukeMasterScript
    Int index = LocalSiloID() + 3
    If master != None && master.CodeData != None && index < master.CodeData.Length
        ObjectReference consoleRef = master.CodeData[index].CardConsole.GetReference()
        EN07_LaunchCardReceptacleScript consoleScript = consoleRef as EN07_LaunchCardReceptacleScript
        If consoleScript != None
            consoleScript.ResetLocalCard()
        EndIf
    EndIf
    SayLocalTopic(EN07_IncorrectCode)
    ResetLocalEntry()
EndFunction

Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || !bPermitActivation
        Return
    EndIf
    If LinkedAccessPanel != None
        If !HasLocalSiloAccess(player)
            SayLocalTopic(EN07_MilitaryPersonelOnly)
            Return
        EndIf
        Quest exteriorMasterQuest = GetOwningQuest()
        EN07_NukeMasterScript exteriorMaster = exteriorMasterQuest as EN07_NukeMasterScript
        If exteriorMaster == None || !exteriorMaster.PrepareLocalSiloEntry(LaunchCardValue, GetReference())
            SayLocalTopic(EN07_MilitaryPersonelOnly)
            Return
        EndIf
        ObjectReference accessPanel = LinkedAccessPanel.GetReference()
        If accessPanel != None
            accessPanel.BlockActivation(False, False)
        EndIf
        SayLocalTopic(EN07_AccessGranted)
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
    Nuke_MasterScript codeMaster = Nuke_Master as Nuke_MasterScript
    If codeMaster == None || !B21:KeypadNative.Ready()
        Debug.Notification("The Tales keypad runtime is unavailable.")
        Return
    EndIf
    codeMaster.UpdateLocalCodeCycle()
    If !HasLocalCodePiece(player)
        SayLocalTopic(EN07_IncorrectCode)
        Return
    EndIf

    bPermitActivation = False
    Int entryCycle = codeMaster.LocalCodeRevision(LocalSiloID())
    ObjectReference entryRef = GetReference()
    Int enteredCode = B21:KeypadNative.ReadCode(entryRef, 8)
    If enteredCode < 0 || GetReference() != entryRef
        bPermitActivation = True
        Return
    EndIf
    If !IsLocalLaunchPrepComplete() || player.GetValue(LaunchCardValue) < 1.0 || player.GetValue(PlayerLaunchCooldown) > Utility.GetCurrentGameTime()
        bPermitActivation = True
        Return
    EndIf
    If !codeMaster.CheckLocalLaunchCode(LocalSiloID(), enteredCode, entryCycle)
        RejectLocalCode()
        Return
    EndIf
    CurrentCode = entryCycle + 1
    player.SetValue(CodeEnteredIndexValue, CurrentCode as Float)
    player.SetValue(CodeEnteredYearValue, Utility.GetCurrentGameTime())
    EN07_EnteredCorrectCode.Show()
    SayLocalTopic(EN07_AccessGranted)

    ObjectReference keypadRef = GetReference()
    If keypadRef != None
        keypadRef.BlockActivation(True, False)
    EndIf
    Quest masterQuest = Game.GetFormFromFile(0x002D0F67, "SeventySix.esm") as Quest
    EN07_NukeMasterScript masterScript = masterQuest as EN07_NukeMasterScript
    If masterScript != None
        masterScript.HandleLocalCodeAccepted(keypadRef)
    EndIf
EndEvent
