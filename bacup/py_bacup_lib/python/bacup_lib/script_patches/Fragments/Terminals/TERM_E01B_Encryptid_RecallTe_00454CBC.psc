Function InitiateRecall()
    Actor player = Game.GetPlayer()
    If player.GetItemCount(P01B_Wolf_RecallKey) < 1
        Return
    EndIf

    If !E01B_Encryptid_Property.IsRunning()
        E01B_Encryptid_StartQuestKeyword.SendStoryEventAndWait(None, player)
    EndIf

    If E01B_Encryptid_Property.IsRunning() && !E01B_Encryptid_Property.IsStageDone(100)
        player.RemoveItem(P01B_Wolf_RecallKey, 1, true)
        E01B_Encryptid_Property.SetStage(100)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    InitiateRecall()
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    InitiateRecall()
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    InitiateRecall()
EndFunction
