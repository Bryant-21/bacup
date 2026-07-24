Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If !W05_MQ_003P_Muscle_SkinnerPerkRank
        Return
    EndIf
    Int rank = Game.GetPlayer().GetValue(W05_MQ_003P_Muscle_SkinnerPerkRank) as Int
    Int found = SkinnerPerks.FindStruct("PerkIndex", rank)
    If found >= 0 && SkinnerPerks[found].PerkToApply
        (akActionRef as Actor).AddPerk(SkinnerPerks[found].PerkToApply)
    EndIf
EndEvent
