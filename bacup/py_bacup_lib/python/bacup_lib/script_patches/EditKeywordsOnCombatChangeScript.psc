Event OnCombatStateChanged(Actor akTarget, int aeCombatState)
    If aeCombatState == InCombat
        AddKeywords(AddKeywordsInCombat)
        RemoveKeywords(RemoveKeywordsInCombat)
    ElseIf aeCombatState == Searching
        AddKeywords(AddKeywordsSearching)
        RemoveKeywords(RemoveKeywordsSearching)
    Else
        AddKeywords(AddKeywordsExitCombat)
        RemoveKeywords(RemoveKeywordsExitCombat)
    EndIf
EndEvent

Function AddKeywords(Keyword[] akList)
    Int i = 0
    While i < akList.Length
        If akList[i]
            Self.AddKeyword(akList[i])
        EndIf
        i += 1
    EndWhile
EndFunction

Function RemoveKeywords(Keyword[] akList)
    Int i = 0
    While i < akList.Length
        If akList[i]
            Self.RemoveKeyword(akList[i])
        EndIf
        i += 1
    EndWhile
EndFunction
