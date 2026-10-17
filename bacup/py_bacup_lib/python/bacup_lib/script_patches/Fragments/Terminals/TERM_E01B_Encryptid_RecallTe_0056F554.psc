Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Actor player = Game.GetPlayer()
    If player.GetItemCount(P01B_Wolf_RecallKey) < 1
        Return
    EndIf

    If !E01B_Encryptid.IsRunning()
        E01B_Encryptid_StartQuestKeyword.SendStoryEventAndWait(None, player)
    EndIf

    If E01B_Encryptid.IsRunning() && !E01B_Encryptid.IsStageDone(100)
        player.RemoveItem(P01B_Wolf_RecallKey, 1, true)
        E01B_Encryptid.SetStage(100)
        If !E01B_Encryptid_MasterQuest.IsRunning()
            E01B_Encryptid_MasterQuest.Start()
        EndIf
        Quests:E01B_Encryptid:TimerScript cooldown = E01B_Encryptid_MasterQuest as Quests:E01B_Encryptid:TimerScript
        If cooldown != None
            cooldown.StartCooldown()
        EndIf
    EndIf
EndFunction
