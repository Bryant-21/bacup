Function Fragment_Stage_0100_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && Appalachia_Dialogue_QuestStartKeyword != None
        If Appalachia_Dialogue_QuestActiveKeyword == None || !playerRef.HasKeyword(Appalachia_Dialogue_QuestActiveKeyword)
            Appalachia_Dialogue_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
