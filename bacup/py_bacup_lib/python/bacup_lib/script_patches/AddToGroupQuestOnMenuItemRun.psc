Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || MenuData == None
        Return
    EndIf

    Int index = 0
    While index < MenuData.Length
        MenuDatum entry = MenuData[index]
        Bool menuMatches = entry.iMenuItemTarget == auiMenuItemID
        Bool playerDoesNotHaveQuest = entry.ActiveQuestKeyword == None || !playerRef.HasKeyword(entry.ActiveQuestKeyword)
        Bool questNeedsStart = entry.QuestToStart == None || !entry.QuestToStart.IsRunning()

        If menuMatches && playerDoesNotHaveQuest && questNeedsStart && entry.StoryEventToSend != None
            entry.StoryEventToSend.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, akTerminalRef, entry.Value1)
            If entry.QuestToStart != None && !entry.QuestToStart.IsRunning()
                entry.QuestToStart.Start()
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent
