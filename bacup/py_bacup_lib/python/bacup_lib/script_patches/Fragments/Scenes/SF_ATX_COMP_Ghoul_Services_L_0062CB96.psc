Function Fragment_Phase_06_End()
    ATX_LiteAllyScript ally = GetOwningQuest() as ATX_LiteAllyScript
    If ally
        ally.GrantLiteAllyBuff(0)
    EndIf
EndFunction
