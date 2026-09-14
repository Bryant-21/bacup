Function Fragment_Stage_0010_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_Started, 1.0)
    EndIf
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(40)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue01, 1.0)
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue02, 1.0)
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue03, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue03, 0.0)
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0231_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue03, 0.0)
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0232_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue03, 0.0)
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue01, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue02, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_CanInteractClue03, 0.0)
    EndIf
    W05_MQ_101P_B_AubrieAliasScript aubrieAlias = Alias_Aubrie as W05_MQ_101P_B_AubrieAliasScript
    If aubrieAlias
        aubrieAlias.PrepareForCave()
    Else
        Actor aubrieRef = Alias_Aubrie.GetActorReference()
        If aubrieRef
            aubrieRef.Enable()
            aubrieRef.EvaluatePackage()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(50)
    Actor aubrieRef = Alias_Aubrie.GetActorReference()
    If aubrieRef
        aubrieRef.Enable()
        aubrieRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_AubrieCanAppearAtFoundation, 1.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieStayedInCave, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieLeftAngry, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieDead, 0.0)
    EndIf
    W05_MQ_101P_B_AubrieAliasScript aubrieAlias = Alias_Aubrie as W05_MQ_101P_B_AubrieAliasScript
    If aubrieAlias
        aubrieAlias.SendHome()
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_AubrieCanAppearAtFoundation, 1.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieStayedInCave, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieLeftAngry, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieDead, 0.0)
    EndIf
    W05_MQ_101P_B_AubrieAliasScript aubrieAlias = Alias_Aubrie as W05_MQ_101P_B_AubrieAliasScript
    If aubrieAlias
        aubrieAlias.SendHome()
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_AubrieCanAppearAtFoundation, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieStayedInCave, 1.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieLeftAngry, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieDead, 0.0)
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0590_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_AubrieCanAppearAtFoundation, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieLeftAngry, 1.0)
    EndIf
    Actor aubrieRef = Alias_Aubrie.GetActorReference()
    If aubrieRef
        aubrieRef.SetValue(Aggression, 2.0)
        aubrieRef.RemoveFromFaction(PlayerFriendFaction)
        aubrieRef.AddToFaction(PlayerEnemyFaction)
        aubrieRef.StartCombat(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_B_AubrieCanAppearAtFoundation, 0.0)
        playerRef.SetValue(W05_MQ_101P_B_AubrieDead, 1.0)
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If W05_MQ_101P && !W05_MQ_101P.IsStageDone(300)
        W05_MQ_101P.SetStage(300)
    EndIf
    ; FO76 quest rewards are server-owned; native CompleteQuest still runs.
EndFunction
