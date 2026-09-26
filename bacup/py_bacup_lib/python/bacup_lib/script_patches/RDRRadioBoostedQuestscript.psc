Function ReconcileStation()
    If !IsRunning() || MTNS01_Intro == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    Bool boosted = MTNS01_Intro.IsStageDone(420) || MTNS01_Intro.IsCompleted()
    If playerRef != None && MTNS01_SignalBoosted_Keyword != None
        boosted = boosted || playerRef.HasKeyword(MTNS01_SignalBoosted_Keyword)
    EndIf
    Bool shouldTransmit = boosted
    ReferenceAlias transmitterAlias = GetAlias(3) as ReferenceAlias
    ObjectReference transmitter = None
    If transmitterAlias != None
        transmitter = transmitterAlias.GetReference()
    EndIf
    Scene broadcast = Game.GetFormFromFile(0x002E66B4, "SeventySix.esm") as Scene
    If shouldTransmit
        If transmitter != None
            transmitter.EnableNoWait()
        EndIf
        If broadcast != None && !broadcast.IsPlaying()
            broadcast.Start()
        EndIf
    Else
        If broadcast != None && broadcast.IsPlaying()
            broadcast.Stop()
        EndIf
        If transmitter != None
            transmitter.DisableNoWait()
        EndIf
    EndIf
EndFunction

Event OnQuestInit()
    If MTNS01_Intro != None
        RegisterForRemoteEvent(MTNS01_Intro, "OnStageSet")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    ReconcileStation()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == MTNS01_Intro
        ReconcileStation()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If MTNS01_Intro != None
        RegisterForRemoteEvent(MTNS01_Intro, "OnStageSet")
    EndIf
    ReconcileStation()
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
EndEvent
