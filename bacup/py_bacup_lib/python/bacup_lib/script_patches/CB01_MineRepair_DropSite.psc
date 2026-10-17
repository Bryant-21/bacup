; CB00 mine repair: once the mine has been inspected (100), activating the drop site with a fusion core in the
; player's inventory feeds one in and finishes the repair (StageToSetIfPlayerHasFusionCore).
Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    Quest owner = GetOwningQuest()
    If akActionRef != player || !owner.IsStageDone(100) || owner.IsStageDone(StageToSetIfPlayerHasFusionCore)
        Return
    EndIf
    If player.GetItemCount(AmmoFusionCore) > 0
        player.RemoveItem(AmmoFusionCore, 1)
        owner.SetStage(StageToSetIfPlayerHasFusionCore)
    EndIf
EndEvent
