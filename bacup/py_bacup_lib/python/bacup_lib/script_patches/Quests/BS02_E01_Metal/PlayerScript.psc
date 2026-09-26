Function ResetEmoteState()
    CancelTimer(601)
    OwningQuest = GetOwningQuest()
    QuestScript = OwningQuest as Quests:BS02_E01_Metal:QuestScript
    PlayerEmoteData = New PlayerEmoteDatum[0]
    SpawnCenterRef = None
EndFunction

B21:B21_TFA_PublicEventController Function EmoteBus()
    Return Game.GetFormFromFile(0xFFF017, "B21_TalesFromAppalachia.esm") as B21:B21_TFA_PublicEventController
EndFunction

Function ArmEmoteListener()
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        RegisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
EndFunction

Event OnAliasInit()
    ResetEmoteState()
    ArmEmoteListener()
EndEvent

Event OnPlayerLoadGame(ObjectReference akSenderRef)
    ArmEmoteListener()
EndEvent

Event OnAliasShutdown()
    CancelTimer(601)
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        UnregisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
    PlayerEmoteData = None
    SpawnCenterRef = None
EndEvent

Int Function FindEmoteDatum(Actor akPlayer)
    If PlayerEmoteData == None
        PlayerEmoteData = New PlayerEmoteDatum[0]
    EndIf
    Int index = PlayerEmoteData.FindStruct("EmotingPlayer", akPlayer)
    If index < 0
        PlayerEmoteDatum datum = New PlayerEmoteDatum
        datum.EmotingPlayer = akPlayer
        datum.CanEmote = True
        PlayerEmoteData.Add(datum)
        index = PlayerEmoteData.Length - 1
    EndIf
    Return index
EndFunction

Bool Function IsNearArena(Actor akPlayer)
    If SpawnCenterRef == None && SpawnCenter != None
        SpawnCenterRef = SpawnCenter.GetReference()
    EndIf
    If SpawnCenterRef == None
        Return False
    EndIf
    Return akPlayer.GetDistance(SpawnCenterRef) <= AllowedDistanceFromSpawnCenter as Float
EndFunction

Function TryEmote(Actor akPlayer)
    If akPlayer == None || akPlayer != Game.GetPlayer()
        Return
    EndIf
    If OwningQuest == None || QuestScript == None
        ResetEmoteState()
    EndIf
    If QuestScript == None || !OwningQuest.IsStageDone(Stage_EmotesRequested) || !QuestScript.IsBonusObjectiveOpen(65)
        Return
    EndIf
    If !IsNearArena(akPlayer)
        Return
    EndIf
    Int index = FindEmoteDatum(akPlayer)
    If !PlayerEmoteData[index].CanEmote
        If CantEmoteNow != None
            CantEmoteNow.Show()
        EndIf
        Return
    EndIf
    PlayerEmoteData[index].CanEmote = False
    StartTimer(EmoteCooldownLength as Float, 601)
    If QuestScript.RegisterEmote() && CrowdReactionSound != None
        CrowdReactionSound.Play(akPlayer)
    EndIf
EndFunction

Event B21:B21_TFA_PublicEventController.B21EmoteV1(B21:B21_TFA_PublicEventController akSender, Var[] akArgs)
    If akArgs.Length != 5
        Return
    EndIf
    Int aiVersion = akArgs[0] as Int
    Actor akPlayer = akArgs[1] as Actor
    String asPlugin = akArgs[2] as String
    Int aiSourceID = akArgs[3] as Int
    Int aiCategoryID = akArgs[4] as Int
    If aiVersion == 1 && asPlugin == "SeventySix.esm" && aiCategoryID == 22337 && (aiSourceID == 1114476 || aiSourceID == 5234353)
        TryEmote(akPlayer)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 601 || PlayerEmoteData == None
        Return
    EndIf
    Int index = 0
    While index < PlayerEmoteData.Length
        PlayerEmoteData[index].CanEmote = True
        index += 1
    EndWhile
EndEvent
