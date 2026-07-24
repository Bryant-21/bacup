Function Fragment_Stage_0100_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && BURN_SQ01 != None && BURN_SQ01_QuestStartKeyword != None
        If !BURN_SQ01.IsRunning() && !BURN_SQ01.IsCompleted()
            BURN_SQ01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
