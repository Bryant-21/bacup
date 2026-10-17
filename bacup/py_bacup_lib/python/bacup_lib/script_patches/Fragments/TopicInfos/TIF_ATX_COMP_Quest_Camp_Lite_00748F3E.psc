; Romance replies (only reachable after INFO 748F38's TIQS sets stage 40). They
; lead into the scene's 5 s Timer phase: cast MySpells[2], RomanceFadeToBlack.
Function Fragment_End(ObjectReference akSpeakerRef)
    ATX_LiteAllyScript ally = GetOwningQuest() as ATX_LiteAllyScript
    If ally
        ally.GrantLiteAllyBuff(2)
    EndIf
EndFunction
