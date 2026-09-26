Event OnAliasInit()
    B21Depositing = False
    ResolveTrough()
EndEvent

Function ResolveTrough()
    OwningQuest = GetOwningQuest()
    QS = OwningQuest as Quests:sfs09:habitatquestscript
    thisHabitat = None
    If QS != None && QS.IsHabitatValid(HabitatIndex)
        thisHabitat = QS.Habitats[HabitatIndex]
    EndIf
EndFunction

Int Function FullStage()
    If HabitatIndex == 0
        Return Stage_AComplete
    ElseIf HabitatIndex == 1
        Return Stage_CComplete
    EndIf
    Return Stage_BComplete
EndFunction

; Depositing moves the habitat's item from the player into the trough, up to the amount the top rank needs.
Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || B21Depositing
        Return
    EndIf
    B21Depositing = True
    ResolveTrough()
    Int oldTier = -1
    Int newTier = -1
    If QS != None && thisHabitat != None && thisHabitat.SpecialItem != None && QS.IsTroughPhaseActive()
        Int needed = QS.NumSpecialItemsNeededForMax - thisHabitat.Count
        Int carried = playerRef.GetItemCount(thisHabitat.SpecialItem)
        If needed > 0 && carried > 0
            oldTier = QS.HabitatTier(HabitatIndex)
            Int amount = carried
            If amount > needed
                amount = needed
            EndIf
            playerRef.RemoveItem(thisHabitat.SpecialItem, amount, False, None)
            Int removed = carried - playerRef.GetItemCount(thisHabitat.SpecialItem)
            If removed > amount
                removed = amount
            EndIf
            If removed > 0
                thisHabitat.Count += removed
                If thisHabitat.Count > QS.NumSpecialItemsNeededForMax
                    thisHabitat.Count = QS.NumSpecialItemsNeededForMax
                EndIf
                QS.UpdateHabitatDisplay(HabitatIndex)
                newTier = QS.HabitatTier(HabitatIndex)
                If thisHabitat.Count >= QS.NumSpecialItemsNeededForMax && !OwningQuest.IsStageDone(FullStage())
                    OwningQuest.SetStage(FullStage())
                EndIf
            EndIf
        EndIf
    EndIf
    B21Depositing = False
    If newTier >= 0 && newTier != oldTier
        QS.SetTroughAnimation(HabitatIndex, newTier)
    EndIf
    ; Every trough at the top rank summons the creatures without waiting for the final call.
    If OwningQuest != None && QS != None && QS.IsTroughPhaseActive() && OwningQuest.IsStageDone(Stage_AComplete) && OwningQuest.IsStageDone(Stage_BComplete) && OwningQuest.IsStageDone(Stage_CComplete)
        OwningQuest.SetStage(Stage_SummonCreatures)
    EndIf
EndEvent
