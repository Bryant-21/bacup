Function SetMarkerEnabled(ReferenceAlias markerAlias, Bool enabled)
    ObjectReference marker = None
    If markerAlias != None
        marker = markerAlias.GetReference()
    EndIf
    If marker != None
        If enabled
            marker.EnableNoWait()
        Else
            marker.DisableNoWait()
        EndIf
    EndIf
EndFunction

Function ReconcileDungeon()
    If BS01_MQ02_Invention == None || BS01_MQ08_Defense == None
        Return
    EndIf
    Bool inventionDone = BS01_MQ02_Invention.IsCompleted() || BS01_MQ02_Invention.IsStageDone(9000)
    Bool defenseDone = BS01_MQ08_Defense.IsCompleted() || BS01_MQ08_Defense.IsStageDone(9000)
    If (BS01_MQ02_Invention.IsRunning() && !inventionDone) || (BS01_MQ08_Defense.IsRunning() && !defenseDone)
        Return
    EndIf
    SetMarkerEnabled(Alias_EnableMarker_BreachClosed, !defenseDone)
    SetMarkerEnabled(Alias_EnableMarker_BreachOpen, defenseDone)
    SetMarkerEnabled(Alias_EnableMarker_SpawnHolePlug, True)
    SetMarkerEnabled(Alias_EnableMarker_EWSInsects, False)
    SetMarkerEnabled(Alias_EnableMarker_EWSRobots, False)
    SetMarkerEnabled(Alias_EnableMarker_AmbientInsects, True)
    SetMarkerEnabled(Alias_EnableMarker_AmbientSuperMutants, False)
    SetMarkerEnabled(Alias_EnableMarker_AmbientMoleMiners, defenseDone)
    If inventionDone
        SetMarkerEnabled(Alias_Activator_UltraciteBattery, False)
        If Alias_Static_DirtSack != None && BS01_MQ02_Invention_DirtSackSwap_AV != None
            ObjectReference dirtSack = Alias_Static_DirtSack.GetReference()
            If dirtSack != None
                SetMarkerEnabled(Alias_Static_DirtSack, dirtSack.GetValue(BS01_MQ02_Invention_DirtSackSwap_AV) > 0.0)
            EndIf
        EndIf
        If !IsStageDone(200)
            SetStage(200)
        EndIf
    EndIf
    If defenseDone && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If BS01_MQ02_Invention != None
        RegisterForRemoteEvent(BS01_MQ02_Invention, "OnStageSet")
    EndIf
    If BS01_MQ08_Defense != None
        RegisterForRemoteEvent(BS01_MQ08_Defense, "OnStageSet")
    EndIf
    ReconcileDungeon()
EndFunction

Function Fragment_Stage_0200_Item_00()
    StartTimer(0.1)
EndFunction

Function Fragment_Stage_0300_Item_00()
    StartTimer(0.1)
EndFunction

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If (akSender == BS01_MQ02_Invention || akSender == BS01_MQ08_Defense) && auiStageID >= 9000
        StartTimer(0.1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0 && IsRunning()
        ReconcileDungeon()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer()
    UnregisterForAllEvents()
EndEvent
