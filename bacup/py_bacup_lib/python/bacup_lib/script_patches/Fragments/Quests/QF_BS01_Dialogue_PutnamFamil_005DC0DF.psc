Function SyncRecruitMarker(ReferenceAlias markerAlias, ActorValue recruitedValue)
    Actor player = Game.GetPlayer()
    If markerAlias == None || recruitedValue == None || player == None
        Return
    EndIf
    ObjectReference marker = markerAlias.GetReference()
    If marker != None
        Bool recruited = player.GetValue(recruitedValue) > 0.0
        If !recruited
            marker.EnableNoWait()
        Else
            marker.DisableNoWait()
        EndIf
    EndIf
EndFunction

Function SyncPutnamMarkers()
    SyncRecruitMarker(Alias_EM_Colin_EnableMarker, BS01_MQ03_FieldTesting_RecruitedColinPutnam_AV)
    SyncRecruitMarker(Alias_EM_Marty_EnableMarker, BS01_MQ03_FieldTesting_RecruitedMartyPutnam_AV)
EndFunction

Function Fragment_Stage_0000_Item_00()
    Quest fieldTesting = Game.GetFormFromFile(0x005C70CD, "SeventySix.esm") as Quest
    If fieldTesting != None
        RegisterForRemoteEvent(fieldTesting, "OnStageSet")
    EndIf
    SyncPutnamMarkers()
EndFunction

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == Game.GetFormFromFile(0x005C70CD, "SeventySix.esm")
        StartTimer(0.1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0 && IsRunning()
        SyncPutnamMarkers()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer()
    UnregisterForAllEvents()
EndEvent
