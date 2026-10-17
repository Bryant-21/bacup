; FO76 rolled one mask per player in PlayerCollectionAlias; the single player loots one from the boss.
Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef == None || FasnachtMaskRewards == None || FasnachtMaskDropToggle == None || FasnachtMaskDropToggle.GetValue() <= 0.0
        Return
    EndIf
    akSenderRef.AddItem(FasnachtMaskRewards)
EndEvent
