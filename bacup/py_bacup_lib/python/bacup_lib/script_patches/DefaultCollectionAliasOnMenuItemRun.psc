Event OnAliasInit()
    Int index = 0
    While MenuData != None && index < MenuData.Length
        Terminal targetTerminal = MenuData[index].TargetTerminal
        If targetTerminal != None
            RegisterForRemoteEvent(targetTerminal, "OnMenuItemRun")
        EndIf
        index += 1
    EndWhile
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If akTerminalRef == None || Find(akTerminalRef) < 0
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    Actor playerRef = Game.GetPlayer()
    If owningQuest == None || playerRef == None
        Return
    EndIf

    Int index = 0
    While MenuData != None && index < MenuData.Length
        MenuDatum menuEntry = MenuData[index]
        Bool senderMatches = menuEntry.TargetTerminal == akSender
        Bool menuMatches = menuEntry.iMenuItemTarget == auiMenuItemID
        Bool prereqMet = menuEntry.iPrereqStage <= 0 || owningQuest.IsStageDone(menuEntry.iPrereqStage)
        Bool stillActive = menuEntry.iShutdownStage <= 0 || owningQuest.GetStage() < menuEntry.iShutdownStage
        Bool ownsQuest = !menuEntry.bCheckForActivePlayer || menuEntry.QuestOwningPlayerAlias == None || menuEntry.QuestOwningPlayerAlias.GetReference() == playerRef
        Bool unblocked = menuEntry.BlockingKeyword == None || !playerRef.HasKeyword(menuEntry.BlockingKeyword)

        If senderMatches && menuMatches && prereqMet && stillActive && ownsQuest && unblocked
            If menuEntry.KeywordToSend != None
                menuEntry.KeywordToSend.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, akTerminalRef)
            ElseIf menuEntry.iStageToSet >= 0 && !owningQuest.IsStageDone(menuEntry.iStageToSet)
                owningQuest.SetStage(menuEntry.iStageToSet)
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnAliasShutdown()
    Int index = 0
    While MenuData != None && index < MenuData.Length
        Terminal targetTerminal = MenuData[index].TargetTerminal
        If targetTerminal != None
            UnregisterForRemoteEvent(targetTerminal, "OnMenuItemRun")
        EndIf
        index += 1
    EndWhile
EndEvent
