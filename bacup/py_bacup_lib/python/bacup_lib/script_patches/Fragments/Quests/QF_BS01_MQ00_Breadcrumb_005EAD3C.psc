Function Fragment_Stage_0100_Item_00()
    If Alias_Loc_FortAtlas != None && LocMountainsObservatoryLocation != None
        Alias_Loc_FortAtlas.ForceLocationTo(LocMountainsObservatoryLocation)
    EndIf
    If Alias_Loc_ATLASInterior != None && LocMountainsObservatoryIntLocation != None
        Alias_Loc_ATLASInterior.ForceLocationTo(LocMountainsObservatoryIntLocation)
    EndIf

    SetObjectiveDisplayed(100)
    TryStartRadio()
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    TryStartTrust()
EndFunction

Function TryStartRadio()
    Bool accepted = False
    ObjectReference playerRef = None

    If BS01_MQ00_Radio != None
        accepted = BS01_MQ00_Radio.IsRunning() || BS01_MQ00_Radio.IsCompleted()
    EndIf
    If !accepted && GetStage() <= 100
        If Alias_Player != None
            playerRef = Alias_Player.GetReference()
        EndIf
        If playerRef != None && BS01_Radio_IntroBroadcast_QuestStartKeyword != None
            accepted = BS01_Radio_IntroBroadcast_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
        If !accepted && BS01_MQ00_Radio != None
            accepted = BS01_MQ00_Radio.IsRunning() || BS01_MQ00_Radio.IsCompleted()
        EndIf
    EndIf

    If !accepted && GetStage() <= 100
        StartTimer(5.0, 100)
    EndIf
EndFunction

Function TryStartTrust()
    Bool accepted = False
    ObjectReference playerRef = None

    If BS01_MQ01_Trust != None
        accepted = BS01_MQ01_Trust.IsRunning() || BS01_MQ01_Trust.IsCompleted()
    EndIf
    If !accepted
        If Alias_Player != None
            playerRef = Alias_Player.GetReference()
        EndIf
        If playerRef != None && BS01_MQ01_Trust_QuestStartKeyword != None
            accepted = BS01_MQ01_Trust_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
        If !accepted && BS01_MQ01_Trust != None
            accepted = BS01_MQ01_Trust.IsRunning() || BS01_MQ01_Trust.IsCompleted()
        EndIf
    EndIf

    If accepted
        If BS01_MQ00_Radio != None && BS01_MQ00_Radio.IsRunning()
            BS01_MQ00_Radio.SetStage(9000)
        EndIf
        Stop()
    Else
        StartTimer(5.0, 300)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 100
        If GetStage() <= 100
            TryStartRadio()
        EndIf
    ElseIf aiTimerID == 300
        If GetStage() >= 300
            TryStartTrust()
        EndIf
    EndIf
EndEvent
