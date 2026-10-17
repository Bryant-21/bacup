; Pioneer Scout badge activity stations (Roboticist, Swimming, Athletics, Archery...): a player whose PioneerScoutRank
; reaches ScoutRankToCheck (and who wears Armor_Required, the swimsuit, where bound) starts the activity by sending its
; StartKeyword story event. An already-running Swimming activity is not restarted.
Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || StartKeyword == None
        Return
    EndIf
    If M01C_Swimming != None && M01C_Swimming.IsRunning()
        Return
    EndIf
    If PioneerScoutRank != None && player.GetValue(PioneerScoutRank) < ScoutRankToCheck
        Return
    EndIf
    If Armor_Required != None && !player.IsEquipped(Armor_Required)
        Return
    EndIf
    StartKeyword.SendStoryEvent(GetCurrentLocation(), Self, player)
EndEvent
