Function Fragment_Stage_0100_Item_00()
    AttemptPenanceHandoff()
EndFunction

Function AttemptPenanceHandoff()
    ObjectReference playerRef = Alias_Player.GetReference()
    Bool accepted = False

    If BS02_MQ01_Penance != None
        accepted = BS02_MQ01_Penance.IsRunning() || BS02_MQ01_Penance.IsCompleted()
    EndIf
    If !accepted && playerRef != None && BS02_MQ01_Penance != None && BS02_MQ01_Penance_StartKeyword != None
        accepted = BS02_MQ01_Penance_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        If !accepted
            accepted = BS02_MQ01_Penance.IsRunning() || BS02_MQ01_Penance.IsCompleted()
        EndIf
    EndIf
    If accepted
        Stop()
    Else
        StartTimer(5.0, 100)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 100 || !IsStageDone(100)
        Return
    EndIf
    AttemptPenanceHandoff()
EndEvent
