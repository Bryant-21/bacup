; Shared scoring for the Mischief Night collections. KeywordToAddOnActivate (MN2_Mischief_BlockActivationKeyword)
; marks a reference as used for this run; mischief only scores while a Mischief Meter is filling.
Event OnAliasInit()
    QuestScript = GetOwningQuest() as MN2_QuestScript
    NumActivated = 0
    Int index = 0
    While index < GetCount()
        ObjectReference target = GetAt(index)
        If target != None
            ResetRef(target)
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || !MischiefOpen() || !ClaimRef(akSenderRef)
        Return
    EndIf
    ScoreMischief(akSenderRef, player, 1.0)
EndEvent

Function ResetRef(ObjectReference akRef)
    If KeywordToAddOnActivate != None && akRef.HasKeyword(KeywordToAddOnActivate)
        akRef.RemoveKeyword(KeywordToAddOnActivate)
    EndIf
    If BlockActivationOnActivate
        akRef.BlockActivation(False)
    EndIf
EndFunction

Bool Function MischiefOpen()
    If QuestScript == None
        QuestScript = GetOwningQuest() as MN2_QuestScript
    EndIf
    Return QuestScript != None && QuestScript.ActivePhase() >= 0
EndFunction

Bool Function ClaimRef(ObjectReference akRef)
    If akRef == None
        Return False
    EndIf
    If KeywordToAddOnActivate != None
        If akRef.HasKeyword(KeywordToAddOnActivate)
            Return False
        EndIf
        akRef.AddKeyword(KeywordToAddOnActivate)
    EndIf
    Return True
EndFunction

Function ScoreMischief(ObjectReference akRef, Actor akPlayer, Float afMultiplier)
    Float score = fMischiefScore
    If MischiefScoreGlobal != None
        score = MischiefScoreGlobal.GetValue()
    EndIf
    QuestScript.AddMischief(score * afMultiplier, akPlayer)
    NumActivated += 1
    ApplyActivationEffects(akRef)
    If ObjectiveToComplete >= 0 && AllClaimed()
        GetOwningQuest().SetObjectiveCompleted(ObjectiveToComplete, True)
    EndIf
EndFunction

Function ApplyActivationEffects(ObjectReference akRef)
    If EnableLinkedRefOnActivate && akRef.GetLinkedRef() != None
        akRef.GetLinkedRef().Enable()
    EndIf
    If BlockActivationOnActivate
        akRef.BlockActivation(True)
    EndIf
    If DestroyRefOnActivate
        akRef.DamageObject(10000.0)
    EndIf
    If DisableRefOnActivate
        akRef.Disable()
    EndIf
    If RemoveFromAliasOnActivate
        RemoveRef(akRef)
    EndIf
    If DeleteRefOnActivate
        akRef.Delete()
    EndIf
EndFunction

Bool Function AllClaimed()
    If KeywordToAddOnActivate == None
        Return False
    EndIf
    Int index = 0
    While index < GetCount()
        ObjectReference target = GetAt(index)
        If target != None && !target.HasKeyword(KeywordToAddOnActivate)
            Return False
        EndIf
        index += 1
    EndWhile
    Return True
EndFunction
