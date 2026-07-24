Function Fragment_Stage_0200_Item_00()
    StartRespondersQuest()
EndFunction

Function Fragment_Stage_0300_Item_00()
    StartRespondersQuest()
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction

Function StartRespondersQuest()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && Quest_Reborn != None && Responders_Keyword != None
        If !Quest_Reborn.IsRunning() && !Quest_Reborn.IsCompleted()
            If Responders_Active_Keyword == None || !playerRef.HasKeyword(Responders_Active_Keyword)
                Responders_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)
            EndIf
        EndIf
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction
