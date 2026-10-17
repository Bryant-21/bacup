; The Overseer's nukes holotape appears when the player comes within SendEventRadius: unless it was already picked up
; (MQ_OverseerHolotapeX4PickedUp) or carried, the start keyword starts MQ_OverseerNukeHolotapeVisible. Leaving past
; ResetRadius re-arms the check.
Event OnLoad()
    ArmProximity()
EndEvent

Function ArmProximity()
    Actor player = Game.GetPlayer()
    If player.GetValue(MQ_OverseerHolotapeX4PickedUp) > 0.0 || player.GetItemCount(MQ_Overseer_12_NukesHolotape) > 0
        Return
    EndIf
    RegisterForDistanceLessThanEvent(player, Self, SendEventRadius)
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    Actor player = Game.GetPlayer()
    If player.GetValue(MQ_OverseerHolotapeX4PickedUp) <= 0.0 && player.GetItemCount(MQ_Overseer_12_NukesHolotape) == 0 && !MQ_OverseerNukeHolotapeVisible.IsRunning()
        MQ_OverseerNukeHolotapeVisible_StartKeyword.SendStoryEvent(None, Self, player)
    EndIf
    If ResetRadius > SendEventRadius
        RegisterForDistanceGreaterThanEvent(player, Self, ResetRadius)
    EndIf
EndEvent

Event OnDistanceGreaterThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    ArmProximity()
EndEvent
