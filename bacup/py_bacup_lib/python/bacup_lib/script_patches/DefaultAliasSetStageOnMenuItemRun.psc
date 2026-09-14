Event OnAliasInit()
    Terminal aliasTerminal = GetAliasTerminal()
    Int index = 0
    While ValueSet != None && index < ValueSet.Length
        Terminal terminalToRegister = ValueSet[index].TargetTerminal
        If terminalToRegister == None
            terminalToRegister = aliasTerminal
        EndIf
        If terminalToRegister != None
            RegisterForRemoteEvent(terminalToRegister, "OnMenuItemRun")
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnAliasShutdown()
    Terminal aliasTerminal = GetAliasTerminal()
    Int index = 0
    While ValueSet != None && index < ValueSet.Length
        Terminal terminalToUnregister = ValueSet[index].TargetTerminal
        If terminalToUnregister == None
            terminalToUnregister = aliasTerminal
        EndIf
        If terminalToUnregister != None
            UnregisterForRemoteEvent(terminalToUnregister, "OnMenuItemRun")
        EndIf
        index += 1
    EndWhile
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    ObjectReference aliasRef = GetReference()
    Terminal aliasTerminal = GetAliasTerminal()
    Int index = 0
    While ValueSet != None && index < ValueSet.Length
        ValueData entry = ValueSet[index]
        Bool senderMatches = entry.TargetTerminal == akSender
        If entry.TargetTerminal == None
            senderMatches = aliasTerminal != None && aliasTerminal == akSender && aliasRef == akTerminalRef
        EndIf
        If senderMatches && entry.iMenuItemTarget == auiMenuItemID
            ApplyValue(entry, akTerminalRef)
        EndIf
        index += 1
    EndWhile
EndEvent

Terminal Function GetAliasTerminal()
    ObjectReference aliasRef = GetReference()
    If aliasRef != None
        Return aliasRef.GetBaseObject() as Terminal
    EndIf
    Return None
EndFunction

Function ApplyValue(ValueData entry, ObjectReference akTerminalRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || entry.iStageToSet < 0
        Return
    EndIf
    If entry.iPrereqStage > 0 && !owningQuest.IsStageDone(entry.iPrereqStage)
        Return
    EndIf
    If entry.iShutdownStage > 0 && owningQuest.GetStage() >= entry.iShutdownStage
        Return
    EndIf
    If entry.bCheckForActivePlayer && entry.QuestOwningPlayerAlias != None && entry.QuestOwningPlayerAlias.GetRef() != Game.GetPlayer()
        Return
    EndIf
    If entry.BlockingKeyword != None && akTerminalRef != None && akTerminalRef.HasKeyword(entry.BlockingKeyword)
        Return
    EndIf
    If entry.KeywordToSend != None
        entry.KeywordToSend.SendStoryEvent(None, Game.GetPlayer(), akTerminalRef)
    EndIf
    If !owningQuest.IsStageDone(entry.iStageToSet)
        owningQuest.SetStage(entry.iStageToSet)
    EndIf
EndFunction
