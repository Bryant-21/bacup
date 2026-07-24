Function Fragment_Stage_0100_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BS02_MQ01_Penance != None && BS02_MQ01_Penance_StartKeyword != None
        If !BS02_MQ01_Penance.IsRunning() && !BS02_MQ01_Penance.IsCompleted()
            BS02_MQ01_Penance_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
