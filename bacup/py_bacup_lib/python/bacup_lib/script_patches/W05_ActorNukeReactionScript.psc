Event OnInit()
    RegisterForLocalNukeEvents()
    RefreshLocalNukeReaction()
EndEvent

Event OnLoad()
    RegisterForLocalNukeEvents()
    RefreshLocalNukeReaction()
EndEvent

Event OnUnload()
    If EN07_MQ_FleeBlast != None
        UnregisterForRemoteEvent(EN07_MQ_FleeBlast, "OnStageSet")
    EndIf
EndEvent

Function RegisterForLocalNukeEvents()
    If EN07_MQ_FleeBlast != None
        UnregisterForRemoteEvent(EN07_MQ_FleeBlast, "OnStageSet")
        RegisterForRemoteEvent(EN07_MQ_FleeBlast, "OnStageSet")
    EndIf
EndFunction

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender != EN07_MQ_FleeBlast
        Return
    EndIf
    If auiStageID == AllClearStage
        HandleLocalNukeAllClear()
    ElseIf auiStageID == 10
        HandleLocalNukeIncoming()
    EndIf
EndEvent

Function RefreshLocalNukeReaction()
    If EN07_MQ_FleeBlast == None
        Return
    EndIf
    If EN07_MQ_FleeBlast.IsStageDone(AllClearStage)
        HandleLocalNukeAllClear()
    ElseIf EN07_MQ_FleeBlast.GetCurrentStageID() == 10
        HandleLocalNukeIncoming()
    EndIf
EndFunction

ObjectReference Function GetLocalNukeBlastMarker()
    If EN07_MQ_FleeBlast == None
        Return None
    EndIf
    ReferenceAlias blastAlias = EN07_MQ_FleeBlast.GetAlias(0) as ReferenceAlias
    If blastAlias == None
        Return None
    EndIf
    Return blastAlias.GetReference()
EndFunction

Function HandleLocalNukeIncoming()
    If bProcessing || (bReactingToNuke && !bStandardEquipQueued) || IsDead() || Self == Game.GetPlayer()
        Return
    EndIf

    ObjectReference blastMarker = GetLocalNukeBlastMarker()
    If blastMarker == None || W05_NPCNukeReactionRadius == None
        Return
    EndIf
    Float reactionRadius = W05_NPCNukeReactionRadius.GetValue()
    If reactionRadius <= 0.0 || blastMarker.GetDistance(Self) > reactionRadius
        Return
    EndIf

    bProcessing = True
    bReactingToNuke = True
    CancelTimer(iBaseOutfitID)
    bStandardEquipQueued = False

    If FleeToNukeLinkLocation && W05_NPCNukeFleeTargetKeyword != None && W05_NPCNukeFleeValue != None
        ObjectReference fleeTarget = GetLinkedRef(W05_NPCNukeFleeTargetKeyword)
        If fleeTarget != None
            SetValue(W05_NPCNukeFleeValue, 1.0)
            EvaluatePackage()
        EndIf
    EndIf

    If SayBombIncomingLine && W05_NukeSettlement_BombIncoming != None && NukeDialogueQuest != None && NukeDialogueQuest.IsRunning()
        Say(W05_NukeSettlement_BombIncoming)
    EndIf

    Outfit hazmatOutfit = HazmatOutfitOverride
    If hazmatOutfit == None
        hazmatOutfit = W05_NPCNuke_HumanHazmatSuitOutfit
    EndIf
    If EquipHazmatSuitDuringNuke && BaseOutfit != None && hazmatOutfit != None && W05_NPCNukeEquipDelayMin != None && W05_NPCNukeEquipDelayMax != None
        bHazmatEquipQueued = True
        StartTimer(Utility.RandomFloat(W05_NPCNukeEquipDelayMin.GetValue(), W05_NPCNukeEquipDelayMax.GetValue()), iHazmatOutfitID)
    EndIf

    bProcessing = False
EndFunction

Function HandleLocalNukeAllClear()
    If bProcessing
        Return
    EndIf
    bProcessing = True

    If bReactingToNuke && FleeToNukeLinkLocation && W05_NPCNukeFleeValue != None
        SetValue(W05_NPCNukeFleeValue, 0.0)
        EvaluatePackage()
    EndIf

    CancelTimer(iHazmatOutfitID)
    bHazmatEquipQueued = False
    If bHazmatOutfitApplied && BaseOutfit != None && !bStandardEquipQueued && W05_NPCNukeUnequipDelayMin != None && W05_NPCNukeUnequipDelayMax != None
        bStandardEquipQueued = True
        StartTimer(Utility.RandomFloat(W05_NPCNukeUnequipDelayMin.GetValue(), W05_NPCNukeUnequipDelayMax.GetValue()), iBaseOutfitID)
    ElseIf !bStandardEquipQueued
        bHazmatOutfitApplied = False
        bReactingToNuke = False
    EndIf

    bProcessing = False
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == iHazmatOutfitID
        bHazmatEquipQueued = False
        If !bReactingToNuke || EN07_MQ_FleeBlast == None || EN07_MQ_FleeBlast.IsStageDone(AllClearStage)
            Return
        EndIf
        Outfit hazmatOutfit = HazmatOutfitOverride
        If hazmatOutfit == None
            hazmatOutfit = W05_NPCNuke_HumanHazmatSuitOutfit
        EndIf
        If hazmatOutfit != None
            SetOutfit(hazmatOutfit)
            bHazmatOutfitApplied = True
        EndIf
    ElseIf aiTimerID == iBaseOutfitID
        bStandardEquipQueued = False
        If EN07_MQ_FleeBlast != None && EN07_MQ_FleeBlast.GetCurrentStageID() == 10
            Return
        EndIf
        If bHazmatOutfitApplied && BaseOutfit != None
            SetOutfit(BaseOutfit)
        EndIf
        bHazmatOutfitApplied = False
        bReactingToNuke = False
    EndIf
EndEvent
