Function Fragment_Stage_0001_Item_00()
    Quest owner = Self as Quest
    GQ_SuperMutantMerchantScript route = owner as GQ_SuperMutantMerchantScript
    If route != None
        route.DebugMoveMerchantToWorkshop()
    EndIf
EndFunction
